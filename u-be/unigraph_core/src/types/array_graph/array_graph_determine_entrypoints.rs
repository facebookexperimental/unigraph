// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::ArrayGraph;
use crate::NodeIDX;

// If we don't have entrypoints explicitly defined,
// we can assume that any node with no parents is an entrypoint
pub fn determine_entrypoints(array_graph: &ArrayGraph) -> Vec<NodeIDX> {
    let mut entrypoints = if let Some(graph_entry_points) = &array_graph.data.entry_points {
        graph_entry_points
            .iter()
            .filter_map(|name| array_graph.data.node_names_ordered.name_to_idx_log(name))
            .collect()
    } else {
        Vec::new()
    };

    // if we had explicitly specified entry points and they are not empty we
    // force return them.
    if !entrypoints.is_empty() {
        return entrypoints;
    }

    // Otherwise we determine entrypoints by scanning Strongly Connected
    // Components (SCCs) and treating each "source" SCC (one with no incoming
    // edge from a node in a *different* SCC) as containing an entrypoint.
    //
    // Operating on SCCs (rather than individual parentless nodes) is what makes
    // isolated cycles work: a cycle with no external parents (e.g. X->Y->Z->X)
    // has no parentless node, but it is a source SCC, so we still pick a
    // representative from it. A normal parentless node is its own size-1 SCC
    // with no reverse edges, so it remains an entrypoint as before.
    //
    // Reachability note: `sccs()` iterates `node_idx_iter_reachable()`, which
    // skips nodes flagged UNREACHABLE. This is safe here because
    // `determine_entrypoints` runs in `apply_traversal_config_to_array_graph`
    // *before* reachability is recomputed, and the derived state (incl. the
    // `sccs` cache) is reset at the end of each apply. So on a freshly built
    // graph all nodes are reachable and every node — including isolated cycles —
    // is covered by an SCC; once a cycle node is chosen as an entrypoint it
    // stays reachable on subsequent traversals.
    let sccs = array_graph.sccs();
    // u32 (not usize) to halve this V-sized allocation on large graphs, matching
    // the NodeIDX width convention. u32::MAX is the "no SCC" sentinel.
    let mut node_to_scc = vec![u32::MAX; array_graph.nodes_len()];
    for (scc_idx, scc) in sccs.iter().enumerate() {
        let scc_idx = scc_idx as u32;
        for &node_idx in scc {
            node_to_scc[node_idx] = scc_idx;
        }
    }

    // We use `edges_configured()` (skips EXCLUDED edges) for the parent scan to
    // stay consistent with how `sccs()` is computed (Tarjan also skips EXCLUDED).
    for (scc_idx, scc) in sccs.iter().enumerate() {
        let scc_idx = scc_idx as u32;
        let is_source_scc = scc.iter().all(|&node_idx| {
            array_graph
                .edges_reverse()
                .edges_configured(node_idx)
                .all(|(parent, _)| node_to_scc[parent] == scc_idx)
        });
        if is_source_scc {
            // Representative = minimum NodeIDX in the SCC, for deterministic output.
            if let Some(&representative) = scc.iter().min() {
                entrypoints.push(representative);
            }
        }
    }

    entrypoints.sort();
    entrypoints
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use anyhow::Result;
    use k9::snapshot;

    use super::*;
    use crate::MapGraph;
    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::idx_to_names;

    #[test]
    fn test_determine_entrypoints() -> Result<()> {
        let mut ag = make_test_array_graph_2()?;

        let entrypoints = determine_entrypoints(&ag);

        snapshot!(
            idx_to_names(&ag, entrypoints),
            r#"
[
    "A",
    "L",
]
"#
        );

        std::sync::Arc::make_mut(&mut ag.data).entry_points =
            Some(vec!["K".to_string()].into_iter().collect());
        let entrypoints = determine_entrypoints(&ag);
        snapshot!(
            idx_to_names(&ag, entrypoints),
            r#"
[
    "K",
]
"#
        );

        std::sync::Arc::make_mut(&mut ag.data).entry_points = Some(BTreeSet::new());
        let entrypoints = determine_entrypoints(&ag);
        snapshot!(
            idx_to_names(&ag, entrypoints),
            r#"
[
    "A",
    "L",
]
"#
        );
        Ok(())
    }

    #[test]
    fn test_determine_entrypoints_isolated_cycle() -> Result<()> {
        // A normal acyclic root (R -> A) plus an isolated cycle (X -> Y -> Z -> X)
        // with no edges into the cycle. The cycle has no parentless node, but it
        // is a source SCC, so exactly one representative (the min-indexed cycle
        // node) must be returned alongside R.
        let json = r#"{
  "nodes": {
    "R": { "metrics": { "size": 1 }, "edges_directed": ["A"] },
    "A": { "metrics": { "size": 1 } },
    "X": { "metrics": { "size": 1 }, "edges_directed": ["Y"] },
    "Y": { "metrics": { "size": 1 }, "edges_directed": ["Z"] },
    "Z": { "metrics": { "size": 1 }, "edges_directed": ["X"] }
  }
}"#;
        let ag = MapGraph::from_json(json)?.to_array_graph(&ll::Task::create_new("test"))?;

        let entrypoints = determine_entrypoints(&ag);
        snapshot!(
            idx_to_names(&ag, entrypoints),
            r#"
[
    "R",
    "X",
]
"#
        );
        Ok(())
    }

    #[test]
    fn test_determine_entrypoints_only_cycle() -> Result<()> {
        // A graph that is *only* an isolated cycle must still yield exactly one
        // representative entrypoint.
        let json = r#"{
  "nodes": {
    "X": { "metrics": { "size": 1 }, "edges_directed": ["Y"] },
    "Y": { "metrics": { "size": 1 }, "edges_directed": ["Z"] },
    "Z": { "metrics": { "size": 1 }, "edges_directed": ["X"] }
  }
}"#;
        let ag = MapGraph::from_json(json)?.to_array_graph(&ll::Task::create_new("test"))?;

        let entrypoints = determine_entrypoints(&ag);
        snapshot!(
            idx_to_names(&ag, entrypoints),
            r#"
[
    "X",
]
"#
        );
        Ok(())
    }
}

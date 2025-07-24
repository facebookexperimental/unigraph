// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::ArrayGraph;
use crate::NodeIDX;

// If we don't have entrypoints explicitly defined,
// we can assume that any node with no parents is an entrypoint
pub fn determine_entrypoints(array_graph: &ArrayGraph) -> Vec<NodeIDX> {
    let mut entrypoints = if let Some(graph_entry_points) = &array_graph.entry_points {
        graph_entry_points
            .iter()
            .filter_map(|name| array_graph.nodes.name_to_idx_log(name))
            .collect()
    } else {
        Vec::new()
    };

    // if we had explicitly specified entry points and they are not empty we
    // force return them.
    if !entrypoints.is_empty() {
        return entrypoints;
    }

    // otherwise we determine them by looking for nodes with no parents
    // NOTE: this will not work when graph has big cycles. We would need
    // to find those we'd need to work on SCCs and find parentless SCCs instead.
    for node_idx in array_graph.node_idx_iter() {
        if array_graph
            .derived_state
            .edges_reverse
            .edges(node_idx)
            .is_empty()
        {
            entrypoints.push(node_idx);
        }
    }
    entrypoints
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use anyhow::Result;
    use k9::snapshot;

    use super::*;
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

        ag.entry_points = Some(vec!["K".to_string()].into_iter().collect());
        let entrypoints = determine_entrypoints(&ag);
        snapshot!(
            idx_to_names(&ag, entrypoints),
            r#"
[
    "K",
]
"#
        );

        ag.entry_points = Some(BTreeSet::new());
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
}

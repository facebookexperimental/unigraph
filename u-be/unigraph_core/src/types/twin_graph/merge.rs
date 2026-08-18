// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::TwinGraph;
use crate::TwinRemap;
use crate::types::twin_graph::NodeDiff;
use crate::types::twin_graph::changed_nodes_graph::ChangedNodesGraph;

pub fn merge_into_twin(
    left: ArrayGraphSerializable,
    right: ArrayGraphSerializable,
    task: &ll::Task,
) -> Result<TwinGraph> {
    let ag_l = left.into_array_graph(task)?;
    let ag_r = right.into_array_graph(task)?;
    let (ag_l, ag_r) = add_super_roots(ag_l, ag_r)?;
    merge_prepared(ag_l, ag_r)
}

/// Give both sides a single, shared entry point.
///
/// Entry points are compared **by name**: the two graphs still live in
/// independent index namespaces at this point, so comparing `NodeIDX` values
/// across them is meaningless whenever the node sets differ.
pub fn add_super_roots(ag_l: ArrayGraph, ag_r: ArrayGraph) -> Result<(ArrayGraph, ArrayGraph)> {
    let entrypoints_left = entrypoint_names(&ag_l);
    let entrypoints_right = entrypoint_names(&ag_r);

    if entrypoints_left == entrypoints_right && entrypoints_left.len() <= 1 {
        return Ok((ag_l, ag_r));
    }

    Ok((ag_l.append_super_root(true)?, ag_r.append_super_root(true)?))
}

fn entrypoint_names(ag: &ArrayGraph) -> BTreeSet<&str> {
    ag.determine_entrypoints()
        .into_iter()
        .map(|idx| ag.idx_to_name(idx))
        .collect()
}

/// Merge two graphs that already share a single entry point and have their
/// traversal applied. See [`add_super_roots`] for the step before this one.
pub fn merge_prepared(ag_l: ArrayGraph, ag_r: ArrayGraph) -> Result<TwinGraph> {
    // Build remap tables by merge-walking the two sorted name lists.
    // No strings are copied — just index arithmetic.
    let remap = TwinRemap::build(&ag_l.data.node_names_ordered, &ag_r.data.node_names_ordered);

    // Compute per-merged-node diff.
    let node_diff = compute_node_diff(&remap, &ag_l, &ag_r);

    let metric_names = ag_l
        .data
        .node_metadata
        .metrics
        .keys()
        .chain(ag_r.data.node_metadata.metrics.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    Ok(TwinGraph {
        remap,
        node_diff,
        metric_names,
        l: ag_l,
        r: ag_r,
        changed_nodes: ChangedNodesGraph::new(),
    })
}

fn compute_node_diff(
    remap: &TwinRemap,
    ag_l: &crate::ArrayGraph,
    ag_r: &crate::ArrayGraph,
) -> Vec<NodeDiff> {
    let flat_metrics = build_flat_metric_pairs(ag_l, ag_r);
    let mut node_diff = vec![NodeDiff::empty(); remap.merged_len];

    for (merged_idx, diff) in node_diff.iter_mut().enumerate().take(remap.merged_len) {
        let merged_node = NodeIDX::from(merged_idx);
        match (remap.twin_to_l[merged_node], remap.twin_to_r[merged_node]) {
            (Some(l_idx), Some(r_idx)) => {
                if has_directed_edge_changes(ag_l, l_idx, ag_r, r_idx, remap)
                    || has_tagged_edges_changes(ag_l, l_idx, ag_r, r_idx, remap)
                    || has_dynamic_edge_changes(ag_l, l_idx, ag_r, r_idx, remap)
                {
                    diff.insert(NodeDiff::EDGES_CHANGED);
                }

                if let Some(flat_metrics) = &flat_metrics {
                    if has_node_metrics_changed(flat_metrics, l_idx, r_idx) {
                        diff.insert(NodeDiff::METRICS_CHANGED);
                    }
                } else {
                    diff.insert(NodeDiff::METRICS_CHANGED);
                }
            }
            (Some(_), None) => diff.mark_not_in_right(),
            (None, Some(_)) => diff.mark_not_in_left(),
            (None, None) => {
                diff.mark_not_in_left();
                diff.mark_not_in_right();
            }
        }
    }

    node_diff
}

/// Compare directed edges by translating targets to merged IDX and sorting.
fn has_directed_edge_changes(
    l: &crate::ArrayGraph,
    l_idx: NodeIDX,
    r: &crate::ArrayGraph,
    r_idx: NodeIDX,
    remap: &TwinRemap,
) -> bool {
    let l_targets = directed_targets_as_merged(l, l_idx, &remap.l_to_twin);
    let r_targets = directed_targets_as_merged(r, r_idx, &remap.r_to_twin);
    l_targets != r_targets
}

/// Get directed (non-tagged, non-dynamic) edge targets for a node, translated to merged IDX.
fn directed_targets_as_merged(
    ag: &crate::ArrayGraph,
    node_idx: NodeIDX,
    to_twin: &[NodeIDX],
) -> Vec<NodeIDX> {
    let mut targets: Vec<NodeIDX> = ag
        .forward_edges(node_idx)
        .filter(|(_target, flags)| {
            !flags.intersects(
                crate::types::array_graph::offset_graph::edge_flags::EdgeFlags::IS_TAGGED
                    | crate::types::array_graph::offset_graph::edge_flags::EdgeFlags::IS_DYNAMIC,
            )
        })
        .map(|(target, _flags)| to_twin[usize::from(target)])
        .collect();
    targets.sort_unstable();
    targets
}

/// Compare tagged edges by translating target IDXes to merged and comparing by tag + targets.
fn has_tagged_edges_changes(
    l: &crate::ArrayGraph,
    l_idx: NodeIDX,
    r: &crate::ArrayGraph,
    r_idx: NodeIDX,
    remap: &TwinRemap,
) -> bool {
    let l_tagged = l.data.edges.tagged_edges_for_node(l_idx);
    let r_tagged = r.data.edges.tagged_edges_for_node(r_idx);

    if l_tagged.is_empty() && r_tagged.is_empty() {
        return false;
    }
    if l_tagged.len() != r_tagged.len() {
        return true;
    }
    // Compare tag-by-tag: same tags must have same targets (in merged IDX)
    for (tag, l_targets) in &l_tagged {
        match r_tagged.get(*tag) {
            None => return true,
            Some(r_targets) => {
                if l_targets.len() != r_targets.len() {
                    return true;
                }
                let l_merged: BTreeSet<NodeIDX> = l_targets
                    .iter()
                    .map(|&idx| remap.l_to_twin[usize::from(idx)])
                    .collect();
                let r_merged: BTreeSet<NodeIDX> = r_targets
                    .iter()
                    .map(|&idx| remap.r_to_twin[usize::from(idx)])
                    .collect();
                if l_merged != r_merged {
                    return true;
                }
            }
        }
    }
    false
}

/// Compare dynamic edges by translating target IDXes to merged.
fn has_dynamic_edge_changes(
    l: &crate::ArrayGraph,
    l_idx: NodeIDX,
    r: &crate::ArrayGraph,
    r_idx: NodeIDX,
    remap: &TwinRemap,
) -> bool {
    let l_dyn = l.data.edges.dynamic_edges_for_node(l_idx);
    let r_dyn = r.data.edges.dynamic_edges_for_node(r_idx);

    if l_dyn.is_empty() && r_dyn.is_empty() {
        return false;
    }
    if l_dyn.len() != r_dyn.len() {
        return true;
    }
    for (type_key, l_edge_map) in &l_dyn {
        match r_dyn.get(*type_key) {
            None => return true,
            Some(r_edge_map) => {
                if l_edge_map.len() != r_edge_map.len() {
                    return true;
                }
                for (edge_name, l_de) in l_edge_map {
                    match r_edge_map.get(*edge_name) {
                        None => return true,
                        Some(r_de) => {
                            if l_de.metadata != r_de.metadata {
                                return true;
                            }
                            if l_de.branches.len() != r_de.branches.len() {
                                return true;
                            }
                            for (branch, l_targets) in &l_de.branches {
                                match r_de.branches.get(*branch) {
                                    None => return true,
                                    Some(r_targets) => {
                                        let l_m: BTreeSet<NodeIDX> = l_targets
                                            .iter()
                                            .map(|&idx| remap.l_to_twin[usize::from(idx)])
                                            .collect();
                                        let r_m: BTreeSet<NodeIDX> = r_targets
                                            .iter()
                                            .map(|&idx| remap.r_to_twin[usize::from(idx)])
                                            .collect();
                                        if l_m != r_m {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Build paired metric arrays for fast comparison.
/// Returns None if the metric names differ between the two graphs.
fn build_flat_metric_pairs<'a>(
    l: &'a crate::ArrayGraph,
    r: &'a crate::ArrayGraph,
) -> Option<Vec<(&'a Vec<f64>, &'a Vec<f64>)>> {
    let l_keys: BTreeSet<_> = l.data.node_metadata.metrics.keys().collect();
    let r_keys: BTreeSet<_> = r.data.node_metadata.metrics.keys().collect();
    if l_keys != r_keys {
        return None;
    }

    let mut pairs = Vec::new();
    for key in &l_keys {
        let l_values = l.data.node_metadata.metrics.get(*key)?;
        let r_values = r.data.node_metadata.metrics.get(*key)?;
        pairs.push((l_values, r_values));
    }
    Some(pairs)
}

fn has_node_metrics_changed(
    flat_metrics: &[(&Vec<f64>, &Vec<f64>)],
    l_idx: NodeIDX,
    r_idx: NodeIDX,
) -> bool {
    for (l_values, r_values) in flat_metrics {
        if l_values[l_idx] != r_values[r_idx] {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::GraphBuilder;
    use crate::tests::test_graphs::make_twin_graph;

    /// Two graphs with one entry point each, but *different* ones.
    ///
    /// The old check compared `Vec<NodeIDX>` across two independent namespaces,
    /// so it saw `[0] == [0]` and skipped the super root — leaving the twin
    /// with two disconnected roots. Comparing names catches it.
    #[test]
    fn differently_named_single_entry_points_get_a_super_root() -> Result<()> {
        let task = ll::Task::create_new("test");
        let one_root = |root: &str| -> Result<ArrayGraphSerializable> {
            let mut builder = GraphBuilder::new();
            builder.add_edge(root, "shared")?;
            builder.build().to_array_graph_serializable()
        };

        let tg = merge_into_twin(one_root("left_root")?, one_root("right_root")?, &task)?;

        let entrypoint_name = |ag: &ArrayGraph| {
            ag.determine_entrypoints()
                .into_iter()
                .map(|idx| ag.idx_to_name(idx).to_string())
                .collect::<Vec<_>>()
        };

        snapshot!(
            (entrypoint_name(&tg.l), entrypoint_name(&tg.r)),
            r#"
(
    [
        "~root~",
    ],
    [
        "~root~",
    ],
)
"#
        );
        Ok(())
    }

    #[test]
    fn test_node_diff() -> Result<()> {
        let tg = make_twin_graph()?;

        let mut result = Vec::new();

        for merged_idx in tg.merged_node_idx_iter() {
            let name = tg.merged_idx_to_name(merged_idx);
            let diff = &tg.node_diff[merged_idx];
            result.push(format!("{}: {}", name, diff.debug()).trim().to_string());
        }

        snapshot!(
            result.join("\n"),
            r#"
A: EDGES_CHANGED | METRICS_CHANGED
B: EDGES_CHANGED
C:
D:
E:
F: EDGES_CHANGED
G:
H:
I:
J: EDGES_CHANGED
K:
L:
M:
N:
O:
P:
Q: ADDED
R: ADDED
S: ADDED
T: ADDED
~root~:
"#
        );
        Ok(())
    }
}

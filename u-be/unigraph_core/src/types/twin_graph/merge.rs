// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;

use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::TwinGraph;
use crate::remap_utils::RemapContext;
use crate::types::array_graph::array_graph_nodes::ArrayGraphNodesForGraphSide;
use crate::types::twin_graph::NodeDiff;
use crate::types::twin_graph::changed_nodes_graph::ChangedNodesGraph;

pub fn merge_into_twin(
    left: ArrayGraphSerializable,
    right: ArrayGraphSerializable,
) -> Result<TwinGraph> {
    let mut ag_l = left.into_array_graph();
    let mut ag_r = right.into_array_graph();

    let entrypoints_left = ag_l.determine_entrypoints();
    let entrypoints_right = ag_r.determine_entrypoints();

    // there can be a case where both graphs have only a single root node
    // and don't need to add a super root, but this root node is different
    // between the two graphs. In this case we need to add a super root
    // to make sure that the root node is the same between the two graphs.
    if (entrypoints_left != entrypoints_right) || (entrypoints_left.len() > 1) {
        ag_l = ag_l.append_super_root(true)?;
        ag_r = ag_r.append_super_root(true)?;
    }

    let left = ag_l.into_serializable();
    let right = ag_r.into_serializable();

    let (node_names, ctx_l, ctx_r) = left.node_names_ordered.merge(&right.node_names_ordered);

    let node_names = Arc::new(node_names);

    let remapped_left = remap_with_nodes(left, &ctx_l, Arc::clone(&node_names))?;
    let remapped_right = remap_with_nodes(right, &ctx_r, Arc::clone(&node_names))?;

    let mut node_diff = vec![NodeDiff::empty(); node_names.combined_nodes_len()];

    let flat_metrics = graph_flat_metric_pairs(&remapped_left, &remapped_right);

    for node_idx in node_names.combined_node_idx_iter() {
        match (
            ctx_l.original_positions[node_idx],
            ctx_r.original_positions[node_idx],
        ) {
            (Some(_), Some(_)) => {
                if has_directed_edge_changes(&remapped_left, &remapped_right, node_idx)
                    || has_tagged_edges_changes(&remapped_left, &remapped_right, node_idx)
                    || has_dynamic_edge_changes(&remapped_left, &remapped_right, node_idx)
                {
                    node_diff[node_idx].insert(NodeDiff::EDGES_CHANGED);
                }

                if let Some(flat_metrics) = &flat_metrics {
                    if has_node_metrics_changed(flat_metrics.clone(), node_idx) {
                        node_diff[node_idx].insert(NodeDiff::METRICS_CHANGED);
                    }
                } else {
                    // If we don't have a valid flat metrics comparison, we assume
                    // that all nodes' metrics changed.
                    node_diff[node_idx].insert(NodeDiff::METRICS_CHANGED);
                }
            }
            (Some(_), None) => node_diff[node_idx].mark_not_in_right(),
            (None, Some(_)) => node_diff[node_idx].mark_not_in_left(),
            (None, None) => {
                node_diff[node_idx].mark_not_in_left();
                node_diff[node_idx].mark_not_in_right();
            }
        }
    }

    let node_diff = Arc::new(node_diff);

    let shared_node_names_l = ArrayGraphNodesForGraphSide::new_with_changes(
        Arc::clone(&node_names),
        Arc::clone(&node_diff),
        crate::GraphSide::Left,
    );
    let shared_node_names_r = ArrayGraphNodesForGraphSide::new_with_changes(
        Arc::clone(&node_names),
        Arc::clone(&node_diff),
        crate::GraphSide::Right,
    );

    let mut left = remapped_left.into_array_graph();
    left.nodes = shared_node_names_l;
    let mut right = remapped_right.into_array_graph();
    right.nodes = shared_node_names_r;

    let metric_names = left
        .metrics
        .keys()
        .chain(right.metrics.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    Ok(TwinGraph {
        node_names,
        node_diff: Arc::clone(&node_diff),
        metric_names,
        l: left,
        r: Some(right),
        changed_nodes: Some(ChangedNodesGraph::new()),
    })
}

fn has_directed_edge_changes(
    left: &ArrayGraphSerializable,
    right: &ArrayGraphSerializable,
    node_idx: NodeIDX,
) -> bool {
    let mut left_edges = left.edges.directed
        [left.edges.directed_offsets[node_idx]..left.edges.directed_offsets[node_idx + 1]]
        .to_vec();
    let mut right_edges = right.edges.directed
        [right.edges.directed_offsets[node_idx]..right.edges.directed_offsets[node_idx + 1]]
        .to_vec();

    // NOTE: this is a potential performance optimization.
    // We sort here to make sure that we don't produce false positives
    // due to different ordering of edges in the two graphs.
    // This does involve allocating new vecs and sorting them, which is pretty
    // heavy. ESPECIALLY when we do it on all edges, which is usually millions.
    //
    // if this becomes a bottleneck we can consider adding a constraint that
    // array graphs MUST have their directed edges always sorted, which will
    // make this comparison O(n) instead of O(n log n) and remove any extra
    // heap allocations.
    left_edges.sort_unstable();
    right_edges.sort_unstable();

    left_edges != right_edges
}

fn has_tagged_edges_changes(
    left: &ArrayGraphSerializable,
    right: &ArrayGraphSerializable,
    node_idx: NodeIDX,
) -> bool {
    let left_edges = &left.edges.tagged.get(&node_idx);
    let right_edges = &right.edges.tagged.get(&node_idx);
    left_edges != right_edges
}

fn has_dynamic_edge_changes(
    left: &ArrayGraphSerializable,
    right: &ArrayGraphSerializable,
    node_idx: NodeIDX,
) -> bool {
    left.edges.dynamic.get(&node_idx) != right.edges.dynamic.get(&node_idx)
}

/// Construct a list of pairs of metric arrays for graphs but only if
/// the set of metric names is identical between the two graphs.
/// If the set of metric names is different we return None, which means every
/// single node's metrics will be considered changed.
fn graph_flat_metric_pairs<'a>(
    left: &'a ArrayGraphSerializable,
    right: &'a ArrayGraphSerializable,
) -> Option<Vec<(&'a Vec<f32>, &'a Vec<f32>)>> {
    let left_metrics = &left.node_metadata.metrics.keys().collect::<BTreeSet<_>>();
    let right_metrics = &right.node_metadata.metrics.keys().collect::<BTreeSet<_>>();
    if left_metrics != right_metrics {
        return None;
    }

    let mut pairs = Vec::new();
    for key in left_metrics {
        let left_values = left.node_metadata.metrics.get(*key)?;
        let right_values = right.node_metadata.metrics.get(*key)?;
        pairs.push((left_values, right_values));
    }
    Some(pairs)
}

fn has_node_metrics_changed(flat_metrics: Vec<(&Vec<f32>, &Vec<f32>)>, node_idx: NodeIDX) -> bool {
    for (l_values, r_values) in flat_metrics {
        if l_values[node_idx] != r_values[node_idx] {
            return true;
        }
    }
    false
}

fn remap_with_nodes(
    graph: ArrayGraphSerializable,
    ctx: &RemapContext,
    shared_node_names: Arc<ArrayGraphNodes>,
) -> Result<ArrayGraphSerializable> {
    Ok(ArrayGraphSerializable {
        node_names_ordered: shared_node_names,
        edges: graph.edges.remap(ctx)?,
        node_metadata: graph.node_metadata.remap(ctx)?,
        graph_settings: graph.graph_settings,
        traversal_config: graph.traversal_config,
        budget_configs: graph.budget_configs,
        entry_points: graph.entry_points,
    })
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;

    #[test]
    fn test_node_diff() -> Result<()> {
        let tg = make_twin_graph()?;

        let mut result = Vec::new();

        for node_idx in tg.node_names.combined_node_idx_iter() {
            let name = tg.node_names.idx_to_name(node_idx);
            let diff = &tg.node_diff[node_idx];
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
\u{10ffff}__root__\u{10ffff}:
"#
        );
        Ok(())
    }
}

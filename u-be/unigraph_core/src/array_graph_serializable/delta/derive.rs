// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use unigraph_delta::Deltable;
use unigraph_delta::OptionDelta;
use unigraph_delta::diff_option;

use super::DirectedEdgeDelta;
use super::DynamicEdgeDelta;
use super::DynamicEdgeSerialized;
use super::DynamicEdgesMap;
use super::GraphDelta;
use super::MetricNodeChange;
use super::NodeEdgeDelta;
use super::TagSetDelta;
use super::TaggedEdgeDelta;
use crate::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::MetricName;
use crate::types::NodeName;
use crate::types::Tag;

/// Compute the delta between two graphs.
///
/// The resulting `GraphDelta` is self-contained (uses node names, not indices)
/// and can be applied to `base` to produce `target`.
pub fn derive_delta(
    base: &ArrayGraphSerializable,
    target: &ArrayGraphSerializable,
) -> Result<GraphDelta> {
    let (merged_nodes, ctx_base, ctx_target) =
        base.node_names_ordered.merge(&target.node_names_ordered);

    let mut nodes_added = Vec::new();
    let mut nodes_removed = Vec::new();
    let mut edge_changes: BTreeMap<NodeName, NodeEdgeDelta> = BTreeMap::new();
    let mut metric_changes: BTreeMap<MetricName, Vec<MetricNodeChange>> = BTreeMap::new();
    let mut tag_set_changes: BTreeMap<NodeName, TagSetDelta> = BTreeMap::new();

    // Remap both graphs to the shared namespace
    let base_remapped = base
        .edges
        .remap(&ctx_base)
        .context("Failed to remap base edges")?;
    let target_remapped = target
        .edges
        .remap(&ctx_target)
        .context("Failed to remap target edges")?;
    let base_metadata_remapped = base
        .node_metadata
        .remap(&ctx_base)
        .context("Failed to remap base metadata")?;
    let target_metadata_remapped = target
        .node_metadata
        .remap(&ctx_target)
        .context("Failed to remap target metadata")?;

    for node_idx in merged_nodes.combined_node_idx_iter() {
        let name = merged_nodes.idx_to_name(node_idx).to_string();
        let in_base = ctx_base.original_positions[node_idx].is_some();
        let in_target = ctx_target.original_positions[node_idx].is_some();

        match (in_base, in_target) {
            (false, true) => {
                // Node added
                nodes_added.push(name.clone());

                // Collect edges for added node
                if let Some(edge_delta) = collect_all_edges_as_added(
                    node_idx,
                    &target_remapped.directed,
                    &target_remapped.directed_offsets,
                    &target_remapped.tagged,
                    &target_remapped.dynamic,
                    &merged_nodes,
                ) {
                    edge_changes.insert(name.clone(), edge_delta);
                }

                // Collect metrics for added node
                collect_metrics_for_node(
                    node_idx,
                    &name,
                    &target_metadata_remapped.metrics,
                    &mut metric_changes,
                );

                // Collect tag sets for added node
                if let Some(ts) =
                    collect_tag_sets_as_added(node_idx, &target_metadata_remapped.tag_sets)
                {
                    tag_set_changes.insert(name, ts);
                }
            }
            (true, false) => {
                // Node removed — edges/metrics implicitly removed
                nodes_removed.push(name);
            }
            (true, true) => {
                // Node in both — diff edges, metrics, tag sets
                if let Some(edge_delta) = diff_edges(
                    node_idx,
                    &base_remapped.directed,
                    &base_remapped.directed_offsets,
                    &base_remapped.tagged,
                    &base_remapped.dynamic,
                    &target_remapped.directed,
                    &target_remapped.directed_offsets,
                    &target_remapped.tagged,
                    &target_remapped.dynamic,
                    &merged_nodes,
                ) {
                    edge_changes.insert(name.clone(), edge_delta);
                }

                diff_metrics_for_node(
                    node_idx,
                    &name,
                    &base_metadata_remapped.metrics,
                    &target_metadata_remapped.metrics,
                    &mut metric_changes,
                );

                if let Some(ts) = diff_tag_sets(
                    node_idx,
                    &base_metadata_remapped.tag_sets,
                    &target_metadata_remapped.tag_sets,
                ) {
                    tag_set_changes.insert(name, ts);
                }
            }
            (false, false) => unreachable!("merge produced a node in neither graph"),
        }
    }

    // Diff top-level settings using field-level deltas via Deltable trait.
    // derive_delta returns None when equal (= no change = Unchanged).
    let graph_settings = base
        .graph_settings
        .derive_delta(&target.graph_settings)
        .unwrap_or(OptionDelta::Unchanged);
    let traversal_config = base
        .traversal_config
        .derive_delta(&target.traversal_config)
        .unwrap_or(OptionDelta::Unchanged);
    let entry_points = diff_option(&base.entry_points, &target.entry_points);

    Ok(GraphDelta {
        nodes_added,
        nodes_removed,
        edge_changes,
        metric_changes,
        tag_set_changes,
        graph_settings,
        traversal_config,
        entry_points,
    })
}

/// Collect all edges for a newly added node as "all added".
fn collect_all_edges_as_added(
    node_idx: NodeIDX,
    directed: &[NodeIDX],
    directed_offsets: &[usize],
    tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    dynamic: &BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    nodes: &ArrayGraphNodes,
) -> Option<NodeEdgeDelta> {
    let dir_delta = {
        let start = directed_offsets[node_idx];
        let end = directed_offsets[node_idx + 1];
        if start < end {
            let added: BTreeSet<NodeName> = directed[start..end]
                .iter()
                .map(|&idx| nodes.idx_to_name(idx).to_string())
                .collect();
            Some(DirectedEdgeDelta {
                added,
                removed: BTreeSet::new(),
            })
        } else {
            None
        }
    };

    let tag_delta = tagged.get(&node_idx).and_then(|tag_map| {
        let target_serialized: BTreeMap<Tag, BTreeSet<NodeName>> = tag_map
            .iter()
            .map(|(tag, targets)| {
                let names: BTreeSet<NodeName> = targets
                    .iter()
                    .map(|&idx| nodes.idx_to_name(idx).to_string())
                    .collect();
                (tag.clone(), names)
            })
            .collect();
        BTreeMap::new().derive_delta(&target_serialized)
    });

    let dyn_delta = dynamic.get(&node_idx).and_then(|edges| {
        let target_serialized = dynamic_edges_to_serialized(edges, nodes);
        DynamicEdgesMap::new().derive_delta(&target_serialized)
    });

    if dir_delta.is_none() && tag_delta.is_none() && dyn_delta.is_none() {
        return None;
    }

    Some(NodeEdgeDelta {
        directed: dir_delta,
        tagged: tag_delta,
        dynamic: dyn_delta,
    })
}

/// Diff edges between base and target for a node that exists in both.
#[allow(clippy::too_many_arguments)]
fn diff_edges(
    node_idx: NodeIDX,
    base_directed: &[NodeIDX],
    base_directed_offsets: &[usize],
    base_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    base_dynamic: &BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    target_directed: &[NodeIDX],
    target_directed_offsets: &[usize],
    target_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    target_dynamic: &BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    nodes: &ArrayGraphNodes,
) -> Option<NodeEdgeDelta> {
    let dir_delta = diff_directed_edges(
        node_idx,
        base_directed,
        base_directed_offsets,
        target_directed,
        target_directed_offsets,
        nodes,
    );

    let tag_delta = diff_tagged_edges(node_idx, base_tagged, target_tagged, nodes);

    let dyn_delta = diff_dynamic_edges(node_idx, base_dynamic, target_dynamic, nodes);

    if dir_delta.is_none() && tag_delta.is_none() && dyn_delta.is_none() {
        return None;
    }

    Some(NodeEdgeDelta {
        directed: dir_delta,
        tagged: tag_delta,
        dynamic: dyn_delta,
    })
}

fn diff_directed_edges(
    node_idx: NodeIDX,
    base_directed: &[NodeIDX],
    base_directed_offsets: &[usize],
    target_directed: &[NodeIDX],
    target_directed_offsets: &[usize],
    nodes: &ArrayGraphNodes,
) -> Option<DirectedEdgeDelta> {
    let base_start = base_directed_offsets[node_idx];
    let base_end = base_directed_offsets[node_idx + 1];
    let target_start = target_directed_offsets[node_idx];
    let target_end = target_directed_offsets[node_idx + 1];

    let base_targets: BTreeSet<NodeIDX> = base_directed[base_start..base_end]
        .iter()
        .copied()
        .collect();
    let target_targets: BTreeSet<NodeIDX> = target_directed[target_start..target_end]
        .iter()
        .copied()
        .collect();

    // added = in target but not base
    let added: BTreeSet<NodeName> = target_targets
        .difference(&base_targets)
        .map(|&idx| nodes.idx_to_name(idx).to_string())
        .collect();

    // removed = in base but not target
    let removed: BTreeSet<NodeName> = base_targets
        .difference(&target_targets)
        .map(|&idx| nodes.idx_to_name(idx).to_string())
        .collect();

    if added.is_empty() && removed.is_empty() {
        None
    } else {
        Some(DirectedEdgeDelta { added, removed })
    }
}

fn diff_tagged_edges(
    node_idx: NodeIDX,
    base_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    target_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    nodes: &ArrayGraphNodes,
) -> Option<TaggedEdgeDelta> {
    let to_serialized =
        |tag_map: &BTreeMap<Tag, BTreeSet<NodeIDX>>| -> BTreeMap<Tag, BTreeSet<NodeName>> {
            tag_map
                .iter()
                .map(|(tag, targets)| {
                    let names: BTreeSet<NodeName> = targets
                        .iter()
                        .map(|&idx| nodes.idx_to_name(idx).to_string())
                        .collect();
                    (tag.clone(), names)
                })
                .collect()
        };

    let base_serialized = base_tagged
        .get(&node_idx)
        .map(to_serialized)
        .unwrap_or_default();
    let target_serialized = target_tagged
        .get(&node_idx)
        .map(to_serialized)
        .unwrap_or_default();

    base_serialized.derive_delta(&target_serialized)
}

fn diff_tag_sets(
    node_idx: NodeIDX,
    base_tag_sets: &BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<Tag>>>,
    target_tag_sets: &BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<Tag>>>,
) -> Option<TagSetDelta> {
    let base = base_tag_sets.get(&node_idx).cloned().unwrap_or_default();
    let target = target_tag_sets.get(&node_idx).cloned().unwrap_or_default();

    base.derive_delta(&target)
}

fn collect_tag_sets_as_added(
    node_idx: NodeIDX,
    tag_sets: &BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<Tag>>>,
) -> Option<TagSetDelta> {
    tag_sets
        .get(&node_idx)
        .and_then(|ts_map| BTreeMap::new().derive_delta(ts_map))
}

fn diff_dynamic_edges(
    node_idx: NodeIDX,
    base_dynamic: &BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    target_dynamic: &BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    nodes: &ArrayGraphNodes,
) -> Option<DynamicEdgeDelta> {
    let base_edges = base_dynamic.get(&node_idx);
    let target_edges = target_dynamic.get(&node_idx);

    let base_serialized = base_edges
        .map(|e| dynamic_edges_to_serialized(e, nodes))
        .unwrap_or_default();
    let target_serialized = target_edges
        .map(|e| dynamic_edges_to_serialized(e, nodes))
        .unwrap_or_default();

    base_serialized.derive_delta(&target_serialized)
}

fn dynamic_edges_to_serialized(
    type_map: &BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    nodes: &ArrayGraphNodes,
) -> DynamicEdgesMap {
    type_map
        .iter()
        .map(|(type_key, edge_map)| {
            let inner = edge_map
                .iter()
                .map(|(edge_name, edge)| {
                    let branches = edge
                        .branches
                        .iter()
                        .map(|(branch, idxs)| {
                            let names: BTreeSet<NodeName> = idxs
                                .iter()
                                .map(|&idx| nodes.idx_to_name(idx).to_string())
                                .collect();
                            (branch.clone(), names)
                        })
                        .collect();
                    (
                        edge_name.clone(),
                        DynamicEdgeSerialized {
                            branches,
                            metadata: edge.metadata.clone(),
                        },
                    )
                })
                .collect();
            (type_key.clone(), inner)
        })
        .collect()
}

fn collect_metrics_for_node(
    node_idx: NodeIDX,
    name: &str,
    metrics: &BTreeMap<MetricName, Vec<f32>>,
    out: &mut BTreeMap<MetricName, Vec<MetricNodeChange>>,
) {
    for (metric_name, values) in metrics {
        let value = values[node_idx];
        if value != 0.0 {
            out.entry(metric_name.clone())
                .or_default()
                .push(MetricNodeChange {
                    node_name: name.to_string(),
                    value,
                });
        }
    }
}

fn diff_metrics_for_node(
    node_idx: NodeIDX,
    name: &str,
    base_metrics: &BTreeMap<MetricName, Vec<f32>>,
    target_metrics: &BTreeMap<MetricName, Vec<f32>>,
    out: &mut BTreeMap<MetricName, Vec<MetricNodeChange>>,
) {
    // All metric names from both sides
    let all_metric_names: BTreeSet<&MetricName> =
        base_metrics.keys().chain(target_metrics.keys()).collect();

    for metric_name in all_metric_names {
        let base_val = base_metrics
            .get(metric_name)
            .map(|v| v[node_idx])
            .unwrap_or(0.0);
        let target_val = target_metrics
            .get(metric_name)
            .map(|v| v[node_idx])
            .unwrap_or(0.0);

        if base_val != target_val {
            out.entry(metric_name.clone())
                .or_default()
                .push(MetricNodeChange {
                    node_name: name.to_string(),
                    value: target_val,
                });
        }
    }
}

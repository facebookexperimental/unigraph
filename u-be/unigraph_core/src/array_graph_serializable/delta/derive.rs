// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use super::DirectedEdgeDelta;
use super::DynamicEdgeDelta;
use super::DynamicEdgeSerialized;
use super::GraphDelta;
use super::MetricNodeChange;
use super::NodeEdgeDelta;
use super::TagSetDelta;
use super::TagSetValueDelta;
use super::TaggedEdgeDelta;
use super::TaggedEdgeTagDelta;
use crate::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
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

    // Diff top-level settings
    let graph_settings = if base.graph_settings != target.graph_settings {
        Some(target.graph_settings.clone())
    } else {
        None
    };
    let traversal_config = if base.traversal_config != target.traversal_config {
        Some(target.traversal_config.clone())
    } else {
        None
    };
    let entry_points = if base.entry_points != target.entry_points {
        Some(target.entry_points.clone())
    } else {
        None
    };

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
    dynamic: &BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
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

    let tag_delta = tagged.get(&node_idx).map(|tag_map| {
        let changes = tag_map
            .iter()
            .map(|(tag, targets)| {
                let added: BTreeSet<NodeName> = targets
                    .iter()
                    .map(|&idx| nodes.idx_to_name(idx).to_string())
                    .collect();
                (
                    tag.clone(),
                    TaggedEdgeTagDelta {
                        added,
                        removed: BTreeSet::new(),
                    },
                )
            })
            .collect();
        TaggedEdgeDelta { changes }
    });

    let dyn_delta = dynamic
        .get(&node_idx)
        .map(|edges| dynamic_edges_to_serialized(edges, nodes));

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
    base_dynamic: &BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
    target_directed: &[NodeIDX],
    target_directed_offsets: &[usize],
    target_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    target_dynamic: &BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
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
    let base_tags = base_tagged.get(&node_idx);
    let target_tags = target_tagged.get(&node_idx);

    match (base_tags, target_tags) {
        (None, None) => None,
        (None, Some(target_map)) => {
            // All tags are new
            let changes = target_map
                .iter()
                .map(|(tag, targets)| {
                    let added: BTreeSet<NodeName> = targets
                        .iter()
                        .map(|&idx| nodes.idx_to_name(idx).to_string())
                        .collect();
                    (
                        tag.clone(),
                        TaggedEdgeTagDelta {
                            added,
                            removed: BTreeSet::new(),
                        },
                    )
                })
                .collect();
            Some(TaggedEdgeDelta { changes })
        }
        (Some(base_map), None) => {
            // All tags are removed
            let changes = base_map
                .iter()
                .map(|(tag, targets)| {
                    let removed: BTreeSet<NodeName> = targets
                        .iter()
                        .map(|&idx| nodes.idx_to_name(idx).to_string())
                        .collect();
                    (
                        tag.clone(),
                        TaggedEdgeTagDelta {
                            added: BTreeSet::new(),
                            removed,
                        },
                    )
                })
                .collect();
            Some(TaggedEdgeDelta { changes })
        }
        (Some(base_map), Some(target_map)) => {
            let mut changes: BTreeMap<Tag, TaggedEdgeTagDelta> = BTreeMap::new();

            // All tags from both sides
            let all_tags: BTreeSet<&Tag> = base_map.keys().chain(target_map.keys()).collect();

            for tag in all_tags {
                let base_set = base_map.get(tag);
                let target_set = target_map.get(tag);

                match (base_set, target_set) {
                    (None, None) => unreachable!(),
                    (None, Some(targets)) => {
                        let added: BTreeSet<NodeName> = targets
                            .iter()
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect();
                        changes.insert(
                            tag.clone(),
                            TaggedEdgeTagDelta {
                                added,
                                removed: BTreeSet::new(),
                            },
                        );
                    }
                    (Some(bases), None) => {
                        let removed: BTreeSet<NodeName> = bases
                            .iter()
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect();
                        changes.insert(
                            tag.clone(),
                            TaggedEdgeTagDelta {
                                added: BTreeSet::new(),
                                removed,
                            },
                        );
                    }
                    (Some(bases), Some(targets)) => {
                        let added: BTreeSet<NodeName> = targets
                            .difference(bases)
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect();
                        let removed: BTreeSet<NodeName> = bases
                            .difference(targets)
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect();
                        if !added.is_empty() || !removed.is_empty() {
                            changes.insert(tag.clone(), TaggedEdgeTagDelta { added, removed });
                        }
                    }
                }
            }

            if changes.is_empty() {
                None
            } else {
                Some(TaggedEdgeDelta { changes })
            }
        }
    }
}

fn diff_dynamic_edges(
    node_idx: NodeIDX,
    base_dynamic: &BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
    target_dynamic: &BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
    nodes: &ArrayGraphNodes,
) -> Option<DynamicEdgeDelta> {
    let base_edges = base_dynamic.get(&node_idx);
    let target_edges = target_dynamic.get(&node_idx);

    match (base_edges, target_edges) {
        (None, None) => None,
        (None, Some(target)) => Some(dynamic_edges_to_serialized(target, nodes)),
        (Some(_), None) => {
            // Dynamic edges removed — replacement with empty vec
            Some(DynamicEdgeDelta {
                replacement: Vec::new(),
            })
        }
        (Some(base), Some(target)) => {
            // Compare: sort and compare. If different, full replacement.
            let mut base_sorted = base.clone();
            let mut target_sorted = target.clone();
            base_sorted.sort_by(cmp_dynamic_edges);
            target_sorted.sort_by(cmp_dynamic_edges);

            if base_sorted == target_sorted {
                None
            } else {
                Some(dynamic_edges_to_serialized(target, nodes))
            }
        }
    }
}

fn cmp_dynamic_edges(a: &ArrayGraphDynamicEdge, b: &ArrayGraphDynamicEdge) -> std::cmp::Ordering {
    a.properties
        .cmp(&b.properties)
        .then_with(|| a.branches.cmp(&b.branches))
}

fn dynamic_edges_to_serialized(
    edges: &[ArrayGraphDynamicEdge],
    nodes: &ArrayGraphNodes,
) -> DynamicEdgeDelta {
    let replacement = edges
        .iter()
        .map(|edge| {
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
            DynamicEdgeSerialized {
                branches,
                properties: edge.properties.clone(),
            }
        })
        .collect();
    DynamicEdgeDelta { replacement }
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

fn diff_tag_sets(
    node_idx: NodeIDX,
    base_tag_sets: &BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<Tag>>>,
    target_tag_sets: &BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<Tag>>>,
) -> Option<TagSetDelta> {
    let base = base_tag_sets.get(&node_idx);
    let target = target_tag_sets.get(&node_idx);

    match (base, target) {
        (None, None) => None,
        (None, Some(target_map)) => {
            let changes = target_map
                .iter()
                .map(|(ts_name, tags)| {
                    (
                        ts_name.clone(),
                        TagSetValueDelta {
                            added: tags.clone(),
                            removed: BTreeSet::new(),
                        },
                    )
                })
                .collect();
            Some(TagSetDelta { changes })
        }
        (Some(base_map), None) => {
            let changes = base_map
                .iter()
                .map(|(ts_name, tags)| {
                    (
                        ts_name.clone(),
                        TagSetValueDelta {
                            added: BTreeSet::new(),
                            removed: tags.clone(),
                        },
                    )
                })
                .collect();
            Some(TagSetDelta { changes })
        }
        (Some(base_map), Some(target_map)) => {
            let mut changes = BTreeMap::new();
            let all_names: BTreeSet<&String> = base_map.keys().chain(target_map.keys()).collect();

            for ts_name in all_names {
                let base_set = base_map.get(ts_name);
                let target_set = target_map.get(ts_name);

                match (base_set, target_set) {
                    (None, None) => unreachable!(),
                    (None, Some(targets)) => {
                        changes.insert(
                            ts_name.clone(),
                            TagSetValueDelta {
                                added: targets.clone(),
                                removed: BTreeSet::new(),
                            },
                        );
                    }
                    (Some(bases), None) => {
                        changes.insert(
                            ts_name.clone(),
                            TagSetValueDelta {
                                added: BTreeSet::new(),
                                removed: bases.clone(),
                            },
                        );
                    }
                    (Some(bases), Some(targets)) => {
                        let added: BTreeSet<Tag> = targets.difference(bases).cloned().collect();
                        let removed: BTreeSet<Tag> = bases.difference(targets).cloned().collect();
                        if !added.is_empty() || !removed.is_empty() {
                            changes.insert(ts_name.clone(), TagSetValueDelta { added, removed });
                        }
                    }
                }
            }

            if changes.is_empty() {
                None
            } else {
                Some(TagSetDelta { changes })
            }
        }
    }
}

fn collect_tag_sets_as_added(
    node_idx: NodeIDX,
    tag_sets: &BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<Tag>>>,
) -> Option<TagSetDelta> {
    tag_sets.get(&node_idx).map(|ts_map| {
        let changes = ts_map
            .iter()
            .map(|(ts_name, tags)| {
                (
                    ts_name.clone(),
                    TagSetValueDelta {
                        added: tags.clone(),
                        removed: BTreeSet::new(),
                    },
                )
            })
            .collect();
        TagSetDelta { changes }
    })
}

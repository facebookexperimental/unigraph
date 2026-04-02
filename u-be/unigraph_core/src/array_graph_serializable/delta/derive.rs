// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use unigraph_delta::Deltable;
use unigraph_delta::MapDelta;
use unigraph_delta::OptionDelta;
use unigraph_delta::SetDelta;

use super::MapGraphDelta;
use crate::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::LabelName;
use crate::types::LabelValue;
use crate::types::MetricName;
use crate::types::NodeName;
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::Tag;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;
use crate::types::map_graph::GraphNodeDelta;

/// Compute the delta between two graphs.
///
/// The resulting `MapGraphDelta` is self-contained (uses node names, not indices)
/// and can be applied to `base` to produce `target`.
pub fn derive_delta(
    base: &ArrayGraphSerializable,
    target: &ArrayGraphSerializable,
) -> Result<MapGraphDelta> {
    let (merged_nodes, ctx_base, ctx_target) =
        base.node_names_ordered.merge(&target.node_names_ordered);

    let mut nodes_added: BTreeMap<NodeName, GraphNode> = BTreeMap::new();
    let mut nodes_removed: BTreeSet<NodeName> = BTreeSet::new();
    let mut nodes_changed: BTreeMap<NodeName, GraphNodeDelta> = BTreeMap::new();

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
                // Node added — build a full GraphNode
                let graph_node = collect_graph_node(
                    node_idx,
                    &target_remapped.directed,
                    &target_remapped.directed_offsets,
                    &target_remapped.tagged,
                    &target_remapped.dynamic,
                    &target_metadata_remapped.metrics,
                    &target_metadata_remapped.labels,
                    &target_metadata_remapped.properties,
                    &merged_nodes,
                );
                nodes_added.insert(name, graph_node);
            }
            (true, false) => {
                nodes_removed.insert(name);
            }
            (true, true) => {
                // Node in both — build GraphNodeDelta if anything changed
                if let Some(node_delta) = diff_graph_node(
                    node_idx,
                    &base_remapped.directed,
                    &base_remapped.directed_offsets,
                    &base_remapped.tagged,
                    &base_remapped.dynamic,
                    &target_remapped.directed,
                    &target_remapped.directed_offsets,
                    &target_remapped.tagged,
                    &target_remapped.dynamic,
                    &base_metadata_remapped.metrics,
                    &target_metadata_remapped.metrics,
                    &base_metadata_remapped.labels,
                    &target_metadata_remapped.labels,
                    &base_metadata_remapped.properties,
                    &target_metadata_remapped.properties,
                    &merged_nodes,
                ) {
                    nodes_changed.insert(name, node_delta);
                }
            }
            (false, false) => unreachable!("merge produced a node in neither graph"),
        }
    }

    // Diff top-level settings
    let graph_settings = base.graph_settings.derive_delta(&target.graph_settings);
    let traversal_config = base.traversal_config.derive_delta(&target.traversal_config);
    let budget_configs = base.budget_configs.derive_delta(&target.budget_configs);
    let entry_points = base.entry_points.derive_delta(&target.entry_points);
    let properties = base.properties.derive_delta(&target.properties);

    let nodes = if nodes_added.is_empty() && nodes_removed.is_empty() && nodes_changed.is_empty() {
        None
    } else {
        Some(MapDelta {
            added: nodes_added,
            removed: nodes_removed,
            changed: nodes_changed,
        })
    };

    Ok(MapGraphDelta {
        nodes,
        graph_settings,
        traversal_config,
        budget_configs,
        entry_points,
        properties,
    })
}

/// Build a full `GraphNode` from an ArrayGraphSerializable's data for a given node.
/// Used for added nodes where we need the complete node data.
#[allow(clippy::too_many_arguments)]
fn collect_graph_node(
    node_idx: NodeIDX,
    directed: &[NodeIDX],
    directed_offsets: &[usize],
    tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    dynamic: &BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    metrics: &BTreeMap<MetricName, Vec<f32>>,
    labels_inverted: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    properties_inverted: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    nodes: &ArrayGraphNodes,
) -> GraphNode {
    // Directed edges
    let start = directed_offsets[node_idx];
    let end = directed_offsets[node_idx + 1];
    let edges_directed = if start < end {
        Some(
            directed[start..end]
                .iter()
                .map(|&idx| nodes.idx_to_name(idx).to_string())
                .collect(),
        )
    } else {
        None
    };

    // Tagged edges
    let edges_tagged = tagged.get(&node_idx).map(|tag_map| {
        tag_map
            .iter()
            .map(|(tag, targets)| {
                (
                    tag.clone(),
                    targets
                        .iter()
                        .map(|&idx| nodes.idx_to_name(idx).to_string())
                        .collect(),
                )
            })
            .collect()
    });

    // Dynamic edges
    let edges_dynamic = dynamic
        .get(&node_idx)
        .map(|type_map| dynamic_edges_to_map_graph(type_map, nodes));

    // Metrics (only include non-zero)
    let node_metrics: BTreeMap<String, f32> = metrics
        .iter()
        .filter_map(|(name, values)| {
            let v = values[node_idx];
            if v != 0.0 {
                Some((name.clone(), v))
            } else {
                None
            }
        })
        .collect();
    let metrics = if node_metrics.is_empty() {
        None
    } else {
        Some(node_metrics)
    };

    // Labels — collect from inverted index
    let node_labels: BTreeMap<String, BTreeSet<String>> = labels_inverted
        .iter()
        .filter_map(|(label_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|values| (label_name.clone(), values.clone()))
        })
        .collect();
    let labels = if node_labels.is_empty() {
        None
    } else {
        Some(node_labels)
    };

    // Properties — collect from inverted index
    let node_properties: BTreeMap<String, String> = properties_inverted
        .iter()
        .filter_map(|(prop_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|value| (prop_name.clone(), value.clone()))
        })
        .collect();
    let properties = if node_properties.is_empty() {
        None
    } else {
        Some(node_properties)
    };

    GraphNode {
        properties,
        labels,
        metrics,
        edges_directed,
        edges_tagged,
        edges_dynamic,
    }
}

/// Diff a single node that exists in both base and target, producing a `GraphNodeDelta`
/// if anything changed.
#[allow(clippy::too_many_arguments)]
fn diff_graph_node(
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
    base_metrics: &BTreeMap<MetricName, Vec<f32>>,
    target_metrics: &BTreeMap<MetricName, Vec<f32>>,
    base_labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    target_labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    base_properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    target_properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    nodes: &ArrayGraphNodes,
) -> Option<GraphNodeDelta> {
    let edges_directed = diff_directed_edges(
        node_idx,
        base_directed,
        base_directed_offsets,
        target_directed,
        target_directed_offsets,
        nodes,
    )
    .map(OptionDelta::Changed);

    let edges_tagged =
        diff_tagged_edges(node_idx, base_tagged, target_tagged, nodes).map(OptionDelta::Changed);

    let edges_dynamic =
        diff_dynamic_edges(node_idx, base_dynamic, target_dynamic, nodes).map(OptionDelta::Changed);

    let metrics = diff_metrics(node_idx, base_metrics, target_metrics).map(OptionDelta::Changed);

    let labels = diff_labels(node_idx, base_labels, target_labels).map(OptionDelta::Changed);

    let properties =
        diff_properties(node_idx, base_properties, target_properties).map(OptionDelta::Changed);

    if edges_directed.is_none()
        && edges_tagged.is_none()
        && edges_dynamic.is_none()
        && metrics.is_none()
        && labels.is_none()
        && properties.is_none()
    {
        return None;
    }

    Some(GraphNodeDelta {
        properties,
        labels,
        metrics,
        edges_directed,
        edges_tagged,
        edges_dynamic,
    })
}

fn diff_directed_edges(
    node_idx: NodeIDX,
    base_directed: &[NodeIDX],
    base_directed_offsets: &[usize],
    target_directed: &[NodeIDX],
    target_directed_offsets: &[usize],
    nodes: &ArrayGraphNodes,
) -> Option<SetDelta<NodeName>> {
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

    let added: BTreeSet<NodeName> = target_targets
        .difference(&base_targets)
        .map(|&idx| nodes.idx_to_name(idx).to_string())
        .collect();
    let removed: BTreeSet<NodeName> = base_targets
        .difference(&target_targets)
        .map(|&idx| nodes.idx_to_name(idx).to_string())
        .collect();

    if added.is_empty() && removed.is_empty() {
        None
    } else {
        Some(SetDelta { added, removed })
    }
}

fn diff_tagged_edges(
    node_idx: NodeIDX,
    base_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    target_tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    nodes: &ArrayGraphNodes,
) -> Option<<BTreeMap<Tag, BTreeSet<NodeName>> as Deltable>::Delta> {
    let to_serialized =
        |tag_map: &BTreeMap<Tag, BTreeSet<NodeIDX>>| -> BTreeMap<Tag, BTreeSet<NodeName>> {
            tag_map
                .iter()
                .map(|(tag, targets)| {
                    (
                        tag.clone(),
                        targets
                            .iter()
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect(),
                    )
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
) -> Option<<BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>> as Deltable>::Delta> {
    let base_serialized = base_dynamic
        .get(&node_idx)
        .map(|e| dynamic_edges_to_map_graph(e, nodes))
        .unwrap_or_default();
    let target_serialized = target_dynamic
        .get(&node_idx)
        .map(|e| dynamic_edges_to_map_graph(e, nodes))
        .unwrap_or_default();

    base_serialized.derive_delta(&target_serialized)
}

fn diff_metrics(
    node_idx: NodeIDX,
    base_metrics: &BTreeMap<MetricName, Vec<f32>>,
    target_metrics: &BTreeMap<MetricName, Vec<f32>>,
) -> Option<<BTreeMap<String, f32> as Deltable>::Delta> {
    let base: BTreeMap<String, f32> = base_metrics
        .iter()
        .filter_map(|(name, values)| {
            let v = values[node_idx];
            if v != 0.0 {
                Some((name.clone(), v))
            } else {
                None
            }
        })
        .collect();
    let target: BTreeMap<String, f32> = target_metrics
        .iter()
        .filter_map(|(name, values)| {
            let v = values[node_idx];
            if v != 0.0 {
                Some((name.clone(), v))
            } else {
                None
            }
        })
        .collect();

    base.derive_delta(&target)
}

/// Diff labels for a single node using the inverted label index.
fn diff_labels(
    node_idx: NodeIDX,
    base_labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    target_labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
) -> Option<<BTreeMap<String, BTreeSet<String>> as Deltable>::Delta> {
    let base: BTreeMap<String, BTreeSet<String>> = base_labels
        .iter()
        .filter_map(|(label_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|values| (label_name.clone(), values.clone()))
        })
        .collect();
    let target: BTreeMap<String, BTreeSet<String>> = target_labels
        .iter()
        .filter_map(|(label_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|values| (label_name.clone(), values.clone()))
        })
        .collect();

    base.derive_delta(&target)
}

/// Diff properties for a single node using the inverted property index.
fn diff_properties(
    node_idx: NodeIDX,
    base_properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    target_properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
) -> Option<<BTreeMap<String, String> as Deltable>::Delta> {
    let base: BTreeMap<String, String> = base_properties
        .iter()
        .filter_map(|(prop_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|value| (prop_name.clone(), value.clone()))
        })
        .collect();
    let target: BTreeMap<String, String> = target_properties
        .iter()
        .filter_map(|(prop_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|value| (prop_name.clone(), value.clone()))
        })
        .collect();

    base.derive_delta(&target)
}

/// Convert ArrayGraphDynamicEdge (NodeIDX-based) to DynamicEdge (name-based).
fn dynamic_edges_to_map_graph(
    type_map: &BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>> {
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
                            (
                                branch.clone(),
                                idxs.iter()
                                    .map(|&idx| nodes.idx_to_name(idx).to_string())
                                    .collect(),
                            )
                        })
                        .collect();
                    (
                        edge_name.clone(),
                        DynamicEdge {
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

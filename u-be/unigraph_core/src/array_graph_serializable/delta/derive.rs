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
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::ArrayGraphSerializableEdges;
use crate::NodeIDX;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::EdgeIDX;
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
        crate::types::twin_graph::twin_remap::merge_node_names(
            &base.node_names_ordered,
            &target.node_names_ordered,
        );

    let mut nodes_added: BTreeMap<NodeName, GraphNode> = BTreeMap::new();
    let mut nodes_removed: BTreeSet<NodeName> = BTreeSet::new();
    let mut nodes_changed: BTreeMap<NodeName, GraphNodeDelta> = BTreeMap::new();

    // Remap both graphs to the shared namespace (4 independent operations)
    let r_base_edges = std::sync::Mutex::new(None);
    let r_target_edges = std::sync::Mutex::new(None);
    let r_base_metadata = std::sync::Mutex::new(None);
    let r_target_metadata = std::sync::Mutex::new(None);

    rayon::scope(|s| {
        s.spawn(|_| {
            *r_base_edges.lock().unwrap() = Some(
                base.edges
                    .remap(&ctx_base)
                    .context("Failed to remap base edges"),
            );
        });
        s.spawn(|_| {
            *r_target_edges.lock().unwrap() = Some(
                target
                    .edges
                    .remap(&ctx_target)
                    .context("Failed to remap target edges"),
            );
        });
        s.spawn(|_| {
            *r_base_metadata.lock().unwrap() = Some(
                base.node_metadata
                    .remap(&ctx_base)
                    .context("Failed to remap base metadata"),
            );
        });
        s.spawn(|_| {
            *r_target_metadata.lock().unwrap() = Some(
                target
                    .node_metadata
                    .remap(&ctx_target)
                    .context("Failed to remap target metadata"),
            );
        });
    });

    let base_remapped = r_base_edges.into_inner().unwrap().unwrap()?;
    let target_remapped = r_target_edges.into_inner().unwrap().unwrap()?;
    let base_metadata_remapped = r_base_metadata.into_inner().unwrap().unwrap()?;
    let target_metadata_remapped = r_target_metadata.into_inner().unwrap().unwrap()?;

    for node_idx in merged_nodes.node_idx_iter() {
        let name = merged_nodes.idx_to_name(node_idx).to_string();
        let in_base = ctx_base.original_positions[node_idx].is_some();
        let in_target = ctx_target.original_positions[node_idx].is_some();

        match (in_base, in_target) {
            (false, true) => {
                // Node added — build a full GraphNode
                let graph_node = collect_graph_node_from_csr(
                    node_idx,
                    &target_remapped,
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
                    &base_remapped,
                    &target_remapped,
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
        entry_points,
        properties,
    })
}

/// Build a full `GraphNode` from unified CSR edges for a given node.
/// Used for added nodes where we need the complete node data.
fn collect_graph_node_from_csr(
    node_idx: NodeIDX,
    edges: &ArrayGraphSerializableEdges,
    metrics: &BTreeMap<MetricName, Vec<f64>>,
    labels_inverted: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    properties_inverted: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    nodes: &ArrayGraphNodes,
) -> GraphNode {
    // Directed edges (no metadata entry)
    let range = edges.edge_range(node_idx);
    let directed: Vec<String> = range
        .clone()
        .filter(|&edge_idx| {
            !edges
                .edge_metadata_map
                .contains_key(&EdgeIDX::from(edge_idx))
        })
        .map(|edge_idx| nodes.idx_to_name(edges.edges[edge_idx]).to_string())
        .collect();
    let edges_directed = if directed.is_empty() {
        None
    } else {
        Some(directed.into_iter().collect())
    };

    // Tagged edges
    let tagged = edges.tagged_edges_for_node(node_idx);
    let edges_tagged = if tagged.is_empty() {
        None
    } else {
        Some(
            tagged
                .into_iter()
                .map(|(tag, targets)| {
                    (
                        tag.to_string(),
                        targets
                            .iter()
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect(),
                    )
                })
                .collect(),
        )
    };

    // Dynamic edges
    let dynamic = edges.dynamic_edges_for_node(node_idx);
    let edges_dynamic = if dynamic.is_empty() {
        None
    } else {
        Some(dynamic_edges_view_to_map_graph(&dynamic, nodes))
    };

    // Metrics (only include non-zero)
    let node_metrics: BTreeMap<String, f64> = metrics
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
    base_edges: &crate::ArrayGraphSerializableEdges,
    target_edges: &crate::ArrayGraphSerializableEdges,
    base_metrics: &BTreeMap<MetricName, Vec<f64>>,
    target_metrics: &BTreeMap<MetricName, Vec<f64>>,
    base_labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    target_labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    base_properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    target_properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    nodes: &ArrayGraphNodes,
) -> Option<GraphNodeDelta> {
    let edges_directed = diff_directed_edges_csr(node_idx, base_edges, target_edges, nodes)
        .map(OptionDelta::Changed);

    let edges_tagged =
        diff_tagged_edges_csr(node_idx, base_edges, target_edges, nodes).map(OptionDelta::Changed);

    let edges_dynamic =
        diff_dynamic_edges_csr(node_idx, base_edges, target_edges, nodes).map(OptionDelta::Changed);

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

fn diff_directed_edges_csr(
    node_idx: NodeIDX,
    base_edges: &crate::ArrayGraphSerializableEdges,
    target_edges: &crate::ArrayGraphSerializableEdges,
    nodes: &ArrayGraphNodes,
) -> Option<SetDelta<NodeName>> {
    let collect_directed = |edges: &crate::ArrayGraphSerializableEdges| -> BTreeSet<NodeIDX> {
        edges
            .edge_range(node_idx)
            .filter(|&edge_idx| {
                !edges
                    .edge_metadata_map
                    .contains_key(&crate::EdgeIDX::from(edge_idx))
            })
            .map(|edge_idx| edges.edges[edge_idx])
            .collect()
    };

    let base_targets = collect_directed(base_edges);
    let target_targets = collect_directed(target_edges);

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

fn diff_tagged_edges_csr(
    node_idx: NodeIDX,
    base_edges: &crate::ArrayGraphSerializableEdges,
    target_edges: &crate::ArrayGraphSerializableEdges,
    nodes: &ArrayGraphNodes,
) -> Option<<BTreeMap<Tag, BTreeSet<NodeName>> as Deltable>::Delta> {
    let to_serialized =
        |edges: &crate::ArrayGraphSerializableEdges| -> BTreeMap<Tag, BTreeSet<NodeName>> {
            edges
                .tagged_edges_for_node(node_idx)
                .into_iter()
                .map(|(tag, targets): (&str, BTreeSet<NodeIDX>)| {
                    (
                        tag.to_string(),
                        targets
                            .iter()
                            .map(|&idx| nodes.idx_to_name(idx).to_string())
                            .collect(),
                    )
                })
                .collect()
        };

    let base_serialized = to_serialized(base_edges);
    let target_serialized = to_serialized(target_edges);
    base_serialized.derive_delta(&target_serialized)
}

fn diff_dynamic_edges_csr(
    node_idx: NodeIDX,
    base_edges: &crate::ArrayGraphSerializableEdges,
    target_edges: &crate::ArrayGraphSerializableEdges,
    nodes: &ArrayGraphNodes,
) -> Option<<BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>> as Deltable>::Delta> {
    let to_serialized = |edges: &crate::ArrayGraphSerializableEdges| -> BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>> {
        edges.dynamic_edges_for_node(node_idx)
            .into_iter()
            .map(|(type_key, edge_map)| {
                let inner = edge_map.into_iter().map(|(edge_name, view)| {
                    (edge_name.to_string(), DynamicEdge {
                        branches: view.branches.into_iter().map(|(b, pts)| {
                            (b.to_string(), pts.iter().map(|pt| nodes.idx_to_name(*pt).to_string()).collect())
                        }).collect(),
                        metadata: view.metadata.cloned(),
                    })
                }).collect();
                (type_key.to_string(), inner)
            })
            .collect()
    };

    let base_serialized = to_serialized(base_edges);
    let target_serialized = to_serialized(target_edges);
    base_serialized.derive_delta(&target_serialized)
}

fn diff_metrics(
    node_idx: NodeIDX,
    base_metrics: &BTreeMap<MetricName, Vec<f64>>,
    target_metrics: &BTreeMap<MetricName, Vec<f64>>,
) -> Option<<BTreeMap<String, f64> as Deltable>::Delta> {
    let base: BTreeMap<String, f64> = base_metrics
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
    let target: BTreeMap<String, f64> = target_metrics
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

/// Convert DynamicEdgeView (borrowed from CSR) to name-based DynamicEdge for delta derivation.
fn dynamic_edges_view_to_map_graph(
    dynamic: &BTreeMap<&str, BTreeMap<&str, crate::array_graph_serializable::DynamicEdgeView<'_>>>,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>> {
    dynamic
        .iter()
        .map(|(&type_key, edge_map)| {
            let inner = edge_map
                .iter()
                .map(|(&edge_name, view)| {
                    let branches = view
                        .branches
                        .iter()
                        .map(|(&branch, idxs)| {
                            (
                                branch.to_string(),
                                idxs.iter()
                                    .map(|&idx| nodes.idx_to_name(idx).to_string())
                                    .collect(),
                            )
                        })
                        .collect();
                    (
                        edge_name.to_string(),
                        DynamicEdge {
                            branches,
                            metadata: view.metadata.cloned(),
                        },
                    )
                })
                .collect();
            (type_key.to_string(), inner)
        })
        .collect()
}

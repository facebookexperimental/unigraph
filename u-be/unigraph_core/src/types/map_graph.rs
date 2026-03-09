// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;

use super::DynamicEdgeName;
use super::DynamicTypeKey;
use super::NodeIDX;
use super::Tag;
use super::TagSetName;
use super::array_graph::ArrayGraph;
use super::array_graph::ArrayGraphDynamicEdge;
use super::array_graph::array_graph_nodes::NodeNamesOrderedBuilder;
use crate::ArrayGraphSerializable;
use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;

type NodeName = String;

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Clone,
    PartialEq,
    Debug,
    unigraph_delta::Deltable
)]
pub struct MapGraph {
    pub nodes: BTreeMap<NodeName, GraphNode>,
    pub traversal_config: Option<TraversalConfig>,
    pub graph_settings: Option<GraphSettings>,

    /// If present, these graph will use these entry points instead
    /// of automatically determining them.
    pub entry_points: Option<BTreeSet<NodeName>>,
}

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Default,
    Clone,
    PartialEq,
    Debug,
    unigraph_delta::Deltable
)]
pub struct GraphNode {
    /// Single-valued string metadata.
    /// e.g. { "oncall": "unigraph", "path": "html/js/..." }
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, String>>,

    /// Multi-valued categorical metadata.
    /// e.g. { "AssertHasteProject": {"comet_pkg", "gemini_pkg"} }
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, BTreeSet<String>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BTreeMap<String, f32>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges_directed: Option<BTreeSet<NodeName>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges_tagged: Option<BTreeMap<String, BTreeSet<NodeName>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges_dynamic: Option<BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>>>,
}

/// Represents an edge that can point to multiple nodes with branches,
/// as well as have metadata associated with it.
#[derive(
    serde::Deserialize,
    serde::Serialize,
    Default,
    Clone,
    PartialEq,
    Debug,
    unigraph_delta::Deltable
)]
pub struct DynamicEdge {
    pub branches: BTreeMap<String, BTreeSet<NodeName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

impl MapGraphDelta {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_none()
            && self.graph_settings.is_none()
            && self.traversal_config.is_none()
            && self.entry_points.is_none()
    }
}

impl MapGraph {
    pub fn to_array_graph(&self) -> Result<ArrayGraph> {
        Ok(self
            .to_array_graph_serializable()
            .context("Failed to convert MapGraph to ArrayGraph")?
            .into_array_graph())
    }

    pub fn to_array_graph_serializable(&self) -> Result<ArrayGraphSerializable> {
        let all_metric_names = self
            .nodes
            .values()
            .flat_map(|n| &n.metrics)
            .flat_map(|m| m.keys())
            .collect::<HashSet<_>>();

        let mut metrics: BTreeMap<String, Vec<f32>> = all_metric_names
            .into_iter()
            .map(|name| (name.clone(), vec![]))
            .collect();

        let mut all_tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>> =
            BTreeMap::new();
        let mut all_node_names_set = self.nodes.keys().cloned().collect::<HashSet<_>>();
        for node in self.nodes.values() {
            all_node_names_set.extend(node.edge_names_iter().cloned());
        }

        let (node_names_ordered, name_to_idx_map) =
            NodeNamesOrderedBuilder::from_names(all_node_names_set);

        let mut directed_edges = vec![];
        let mut directed_offsets = vec![0];

        let mut all_tagged_edges: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>> =
            BTreeMap::new();
        let mut all_dynamic_edges: BTreeMap<
            NodeIDX,
            BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
        > = BTreeMap::new();

        for node_name in node_names_ordered.combined_node_names_iter() {
            let node_idx = *name_to_idx_map
                .get(node_name)
                .with_context(|| format!("Node name not found: {node_name}"))?;

            // The node might not be there if there was an edge to a node that was not in the graph.
            if let Some(node) = self.nodes.get(node_name) {
                for (tag, edges) in node.edges_tagged.iter().flatten() {
                    all_tagged_edges.entry(node_idx).or_default().insert(
                        tag.clone(),
                        edges
                            .iter()
                            .filter_map(|edge| name_to_idx_map.get(edge))
                            .copied()
                            .collect::<BTreeSet<_>>(),
                    );
                }

                for (type_key, edge_map) in node.edges_dynamic.iter().flatten() {
                    for (edge_name, dynamic) in edge_map {
                        let mut result = ArrayGraphDynamicEdge {
                            metadata: dynamic.metadata.clone(),
                            branches: BTreeMap::new(),
                        };

                        for (branch_name, edges) in dynamic.branches.iter() {
                            result.branches.insert(
                                branch_name.clone(),
                                edges
                                    .iter()
                                    .filter_map(|edge| name_to_idx_map.get(edge))
                                    .copied()
                                    .collect(),
                            );
                        }
                        all_dynamic_edges
                            .entry(node_idx)
                            .or_default()
                            .entry(type_key.clone())
                            .or_default()
                            .insert(edge_name.clone(), result);
                    }
                }

                for directed_edge in node.edges_directed.iter().flatten() {
                    let points_to = name_to_idx_map.get(directed_edge).copied().with_context(
                        || {
                            format!(
                                "Directed edge points to a node not in the graph: {directed_edge}"
                            )
                        },
                    )?;
                    directed_edges.push(points_to);
                }
                directed_offsets.push(directed_edges.len());

                for (metrc_name, metric_values) in metrics.iter_mut() {
                    if let Some(metric_value) =
                        node.metrics.as_ref().and_then(|m| m.get(metrc_name))
                    {
                        metric_values.push(*metric_value);
                    } else {
                        metric_values.push(0.0);
                    }
                }
                if let Some(labels) = &node.labels {
                    all_tag_sets.insert(node_idx, labels.clone());
                }
            } else {
                directed_offsets.push(directed_edges.len());
                for (_metric_name, metric_values) in metrics.iter_mut() {
                    metric_values.push(0.0);
                }
            }
        }

        Ok(ArrayGraphSerializable {
            node_names_ordered: Arc::new(node_names_ordered),
            edges: crate::ArrayGraphSerializableEdges {
                directed: directed_edges,
                directed_offsets,
                tagged: all_tagged_edges,
                dynamic: all_dynamic_edges,
            },
            node_metadata: crate::ArrayGraphSerializableNodeMetadata {
                metrics,
                tag_sets: all_tag_sets,
            },
            graph_settings: self.graph_settings.clone(),
            traversal_config: self.traversal_config.clone(),
            entry_points: self.entry_points.clone(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let graph: MapGraph = serde_json::from_str(json).context("Failed to parse JSON")?;
        Ok(graph)
    }

    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string(self).context("Failed to serialize")?;
        Ok(json)
    }
}

impl GraphNode {
    pub fn directed_edges_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.edges_directed
            .as_ref()
            .map(|edges| edges.iter())
            .unwrap_or_default()
    }

    pub fn tagged_edge_names_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.edges_tagged
            .as_ref()
            .into_iter()
            .flat_map(|m| m.values())
            .flat_map(|v| v.iter())
    }

    pub fn dynamic_edge_names_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.edges_dynamic
            .as_ref()
            .into_iter()
            .flat_map(|type_map| type_map.values())
            .flat_map(|edge_map| edge_map.values())
            .flat_map(|edge| edge.branches.values().flat_map(|v| v.iter()))
    }

    pub fn edge_names_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.directed_edges_iter()
            .chain(self.tagged_edge_names_iter())
            .chain(self.dynamic_edge_names_iter())
    }

    pub fn arrow_iter<'a>(&'a self) -> impl Iterator<Item = NamedArrow> + 'a {
        let directed = self.directed_edges_iter().map(|points_to| NamedArrow {
            tag: None,
            dynamic: None,
            points_to: points_to.clone(),
        });

        let tagged = self
            .edges_tagged
            .as_ref()
            .into_iter()
            .flatten()
            .map(|(tag, edges)| {
                edges.iter().map(move |points_to| NamedArrow {
                    tag: Some(tag.clone()),
                    dynamic: None,
                    points_to: points_to.clone(),
                })
            });

        let dynamic = self
            .edges_dynamic
            .as_ref()
            .into_iter()
            .flat_map(|type_map| {
                type_map.iter().flat_map(|(type_key, edge_map)| {
                    edge_map.iter().flat_map(move |(edge_name, d)| {
                        d.branches.iter().flat_map(move |(branch, edges)| {
                            edges.iter().map(move |points_to| NamedArrow {
                                tag: None,
                                dynamic: Some(DynamicEdgeInfo {
                                    type_key: type_key.clone(),
                                    edge_name: edge_name.clone(),
                                    branch: branch.clone(),
                                    metadata: d.metadata.clone(),
                                }),
                                points_to: points_to.clone(),
                            })
                        })
                    })
                })
            });

        directed.chain(tagged.flatten()).chain(dynamic)
    }
}

/// Dynamic-edge-only fields. None for directed/tagged edges.
pub struct DynamicEdgeInfo {
    pub type_key: DynamicTypeKey,
    pub edge_name: DynamicEdgeName,
    pub branch: String,
    pub metadata: Option<BTreeMap<String, String>>,
}

/// Version of an Arrow that uses node name string instead of NodeIDX.
pub struct NamedArrow {
    pub tag: Option<String>,
    pub dynamic: Option<DynamicEdgeInfo>,
    pub points_to: NodeName,
}

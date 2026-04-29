// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;

use anyhow::Context;
use anyhow::Result;

use super::DynamicEdgeName;
use super::DynamicTypeKey;
use super::LabelName;
use super::LabelValue;
use super::NodeIDX;
use super::PropertyName;
use super::PropertyValue;
use super::array_graph::ArrayGraph;
use super::array_graph::array_graph_nodes::NodeNamesOrderedBuilder;
use crate::ArrayGraphSerializable;
use crate::EdgeMeta;
use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;
use crate::types::EdgeIDX;
use crate::types::EdgeMetaIDX;

type NodeName = String;

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Clone,
    PartialEq,
    Debug,
    unigraph_delta::Deltable,
    typegen::TypeGen
)]
pub struct MapGraph {
    pub nodes: BTreeMap<NodeName, GraphNode>,
    pub traversal_config: Option<TraversalConfig>,
    pub graph_settings: Option<GraphSettings>,

    /// If present, these graph will use these entry points instead
    /// of automatically determining them.
    pub entry_points: Option<BTreeSet<NodeName>>,

    /// Graph-level key-value properties (not per-node).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

/// Large MapGraphs can take tens of seconds to deallocate due to the
/// deeply nested BTreeMap<String, GraphNode> structure. Moving the heavy
/// fields to a background thread keeps the caller's hot path fast, and on
/// process exit the OS reclaims everything without running destructors.
///
/// See: https://abrams.cc/rust-dropping-things-in-another-thread
impl Drop for MapGraph {
    fn drop(&mut self) {
        let nodes = std::mem::take(&mut self.nodes);
        if nodes.len() > 1000 {
            std::thread::spawn(move || drop(nodes));
        }
    }
}

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Default,
    Clone,
    PartialEq,
    Debug,
    unigraph_delta::Deltable,
    typegen::TypeGen
)]
pub struct GraphNode {
    /// Single-valued string metadata.
    /// e.g. { "oncall": "unigraph", "path": "html/js/..." }
    ///
    /// **Performance note:** Properties are stored as heap-allocated JSON
    /// (`BTreeMap<String, String>`) and converted to per-property sparse maps
    /// in the ArrayGraph. They require pointer chasing and are significantly
    /// more expensive than metrics or edges. Avoid storing information that
    /// is derivable from the node name or graph structure (e.g. don't store
    /// `"path"` if the node name already contains the path). Prefer encoding
    /// categorical data as graph structure (entry-point nodes + edges) over
    /// properties when the data can be derived by traversal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, String>>,

    /// Multi-valued categorical metadata.
    /// e.g. { "AssertHasteProject": {"comet_pkg", "gemini_pkg"} }
    ///
    /// **Performance note:** Labels are the most expensive per-node field.
    /// Each label is a `BTreeSet<String>` of heap-allocated strings, stored
    /// in a `BTreeMap` — nested heap allocations with poor cache locality.
    /// In the ArrayGraph, labels become sparse maps of `NodeIDX → BTreeSet`.
    /// A single widely-shared label (e.g. route membership with 70+ values
    /// on every shared module) can dominate serialization size and memory.
    /// Prefer encoding membership as graph structure: create synthetic group
    /// nodes with edges to members, then derive membership by reverse DFS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, BTreeSet<String>>>,

    /// Numeric per-node values (e.g. file size in bytes).
    /// Cheap: stored as flat `Vec<f32>` per metric in the ArrayGraph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BTreeMap<String, f32>>,

    /// Untagged directed edges. Cheap: stored in CSR (flat array + offsets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges_directed: Option<BTreeSet<NodeName>>,

    /// Tagged directed edges. Cheap: same CSR storage, with an EdgeMeta
    /// entry per tag.
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
    unigraph_delta::Deltable,
    typegen::TypeGen
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
            && self.properties.is_none()
    }
}

impl MapGraph {
    pub fn to_array_graph(&self, task: &ll::Task) -> Result<ArrayGraph> {
        self.to_array_graph_serializable()
            .context("Failed to convert MapGraph to ArrayGraph")?
            .into_array_graph(task)
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

        let mut all_labels: BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>> =
            BTreeMap::new();
        let mut all_properties: BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>> =
            BTreeMap::new();
        let mut all_node_names_set = self.nodes.keys().cloned().collect::<HashSet<_>>();
        for node in self.nodes.values() {
            all_node_names_set.extend(node.edge_names_iter().cloned());
        }

        let (node_names_ordered, name_to_idx_map) =
            NodeNamesOrderedBuilder::from_names(all_node_names_set);

        // Unified CSR: directed first, then tagged (sorted by tag+target),
        // then dynamic (sorted by type_key+edge_name+branch+target).
        let mut edges: Vec<NodeIDX> = vec![];
        let mut edge_offsets: Vec<usize> = vec![0];
        let mut edge_metadata: Vec<EdgeMeta> = vec![];
        let mut edge_metadata_map: BTreeMap<EdgeIDX, EdgeMetaIDX> = BTreeMap::new();

        for node_name in node_names_ordered.node_names_iter() {
            let node_idx = *name_to_idx_map
                .get(node_name)
                .with_context(|| format!("Node name not found: {node_name}"))?;

            if let Some(node) = self.nodes.get(node_name) {
                // 1. Directed edges
                for directed_edge in node.edges_directed.iter().flatten() {
                    let points_to = name_to_idx_map.get(directed_edge).copied().with_context(
                        || {
                            format!(
                                "Directed edge points to a node not in the graph: {directed_edge}"
                            )
                        },
                    )?;
                    edges.push(points_to);
                }

                // 2. Tagged edges (sorted by tag name, then target)
                for (tag, tag_edges) in node.edges_tagged.iter().flatten() {
                    let meta_idx = EdgeMetaIDX::from(edge_metadata.len());
                    edge_metadata.push(EdgeMeta::Tagged { tag: tag.clone() });
                    for target_name in tag_edges {
                        if let Some(&target_idx) = name_to_idx_map.get(target_name) {
                            let edge_idx = EdgeIDX::from(edges.len());
                            edges.push(target_idx);
                            edge_metadata_map.insert(edge_idx, meta_idx);
                        }
                    }
                }

                // 3. Dynamic edges (sorted by type_key, then edge_name)
                for (type_key, edge_map) in node.edges_dynamic.iter().flatten() {
                    for (edge_name, dynamic) in edge_map {
                        for (branch_name, branch_edges) in &dynamic.branches {
                            let meta_idx = EdgeMetaIDX::from(edge_metadata.len());
                            edge_metadata.push(EdgeMeta::Dynamic {
                                type_key: type_key.clone(),
                                edge_name: edge_name.clone(),
                                branch: branch_name.clone(),
                                metadata: dynamic.metadata.clone(),
                            });
                            for target_name in branch_edges {
                                if let Some(&target_idx) = name_to_idx_map.get(target_name) {
                                    let edge_idx = EdgeIDX::from(edges.len());
                                    edges.push(target_idx);
                                    edge_metadata_map.insert(edge_idx, meta_idx);
                                }
                            }
                        }
                    }
                }

                edge_offsets.push(edges.len());

                for (metric_name, metric_values) in metrics.iter_mut() {
                    if let Some(metric_value) =
                        node.metrics.as_ref().and_then(|m| m.get(metric_name))
                    {
                        metric_values.push(*metric_value);
                    } else {
                        metric_values.push(0.0);
                    }
                }
                if let Some(labels) = &node.labels {
                    for (label_name, label_values) in labels {
                        all_labels
                            .entry(label_name.clone())
                            .or_default()
                            .insert(node_idx, label_values.clone());
                    }
                }
                if let Some(properties) = &node.properties {
                    for (prop_name, prop_value) in properties {
                        all_properties
                            .entry(prop_name.clone())
                            .or_default()
                            .insert(node_idx, prop_value.clone());
                    }
                }
            } else {
                edge_offsets.push(edges.len());
                for (_metric_name, metric_values) in metrics.iter_mut() {
                    metric_values.push(0.0);
                }
            }
        }

        Ok(ArrayGraphSerializable {
            node_names_ordered,
            edges: crate::ArrayGraphSerializableEdges {
                edges,
                edge_offsets,
                edge_metadata,
                edge_metadata_map,
            },
            node_metadata: crate::ArrayGraphSerializableNodeMetadata {
                metrics,
                labels: all_labels,
                properties: all_properties,
            },
            graph_settings: self.graph_settings.clone(),
            traversal_config: self.traversal_config.clone(),
            entry_points: self.entry_points.clone(),
            properties: self.properties.clone(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let graph: MapGraph = serde_json::from_str(json).context("Failed to parse JSON")?;
        Ok(graph)
    }

    pub fn from_json_bytes(json: &[u8]) -> Result<Self> {
        serde_json::from_slice(json).context("Failed to parse MapGraph from JSON bytes")
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

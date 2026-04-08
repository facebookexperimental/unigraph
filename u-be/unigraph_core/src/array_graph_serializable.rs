// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Serializable representation of an [`ArrayGraph`].
//!
//! This module defines [`ArrayGraphSerializable`], a format-agnostic snapshot
//! of the graph that can be round-tripped through JSON (or any serde format)
//! and converted back into a live [`ArrayGraph`].
//!
//! ## Sub-modules
//!
//! - [`delta`] — incremental graph updates (add/remove nodes and edges).
//! - [`package`] — chunked, ZSTD-compressed blob packaging for efficient
//!   storage and transport (see [`ArrayGraphSerializablePackage`]).

pub mod delta;
pub mod error_package;
pub(crate) mod package;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use rayon::prelude::*;

use super::ArrayGraph;
use super::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializablePackage;
use crate::ArrayGraphSerializablePackageConfig;
use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;
use crate::remap_utils::RemapContext;
use crate::remap_utils::remap_edges;
use crate::remap_utils::remap_node_metadata;
use crate::remap_utils::remap_node_names_ordered;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::LabelName;
use crate::types::LabelValue;
use crate::types::MetricName;
use crate::types::NodeIDX;
use crate::types::NodeName;
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::Tag;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_nodes::ArrayGraphNodesForGraphSide;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;
use crate::types::array_graph::offset_graph::OffsetGraph;

/// A serializable representation of an array graph, which can be used for
/// storing or transmitting the graph structure.
///
/// IMPORTANT: When adding or removing fields here, you MUST update ALL of
/// the following to maintain field parity:
///   - `ArrayGraph` struct (array_graph.rs)
///   - `From<ArrayGraph> for ArrayGraphSerializable`
///   - `From<ArrayGraphSerializable> for ArrayGraph`
///   - `ArrayGraphSerializable::remap()`
///   - `ManifestBlobs` struct, `pack()`, and `unpack()` (package.rs)
///   - `ManifestBlobs::get_all_blob_ids()`
///   - `apply_deltas()` (delta/apply.rs)
///   - `remap_with_nodes()` (twin_graph/merge.rs)
///   - `MapGraph::to_array_graph_serializable()` (map_graph.rs)
///   - `super_root::append_super_root()` destructure + reconstruction
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone)]
pub struct ArrayGraphSerializable {
    pub node_names_ordered: Arc<ArrayGraphNodes>,
    pub edges: ArrayGraphSerializableEdges,
    pub node_metadata: ArrayGraphSerializableNodeMetadata,

    pub graph_settings: Option<GraphSettings>,
    pub traversal_config: Option<TraversalConfig>,

    /// If present, these graph will use these entrypoints instead
    /// of automatically determining them.
    pub entry_points: Option<BTreeSet<NodeName>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

/// Serializable edge data for an array graph.
///
/// Directed edges are stored in a CSR (Compressed Sparse Row) layout:
/// `directed` is a flat list of target node indices and `directed_offsets`
/// provides per-source-node boundaries into that list.
///
/// Tagged and dynamic edges use map-based representations since they carry
/// additional metadata (tags, branch labels, properties).
///
/// Note: when serialized, only "pure" directed edges are included in the CSR
/// arrays — tagged and dynamic edges are excluded to avoid duplication, since
/// they are stored separately. On deserialization the full offset graph is
/// reconstructed by merging all three edge types back together.
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone)]
pub struct ArrayGraphSerializableEdges {
    /// Flat list of directed-edge target node indices.
    pub directed: Vec<NodeIDX>,
    /// CSR offsets into `directed` — `directed[directed_offsets[i]..directed_offsets[i+1]]`
    /// gives the targets for source node `i`.
    pub directed_offsets: Vec<usize>,
    /// Tagged edges: source node → tag → set of target nodes.
    pub tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    /// Dynamic edges with runtime-defined branches and metadata.
    pub dynamic: BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
}

impl ArrayGraphSerializableEdges {
    /// Remaps all node indices in this edge set according to the given context.
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_edges(self, ctx).context("Failed to remap ArrayGraphSerializableEdges")
    }
}

/// Serializable per-node metadata: numeric metrics, categorical labels, and string properties.
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone)]
pub struct ArrayGraphSerializableNodeMetadata {
    /// Named metrics — each entry maps a metric name to a `Vec<f32>` with one
    /// value per node (indexed by [`NodeIDX`]).
    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    /// Per-label-name index — maps a label name to the set of nodes that have it,
    /// and for each node the set of values for that label.
    pub labels: BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    /// Per-property-name index — maps a property name to the set of nodes that have it,
    /// and for each node the single value for that property.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
}

impl ArrayGraphSerializableNodeMetadata {
    /// Remaps all node indices in the metadata according to the given context.
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_node_metadata(self, ctx).context("Failed to remap ArrayGraphSerializableNodeMetadata")
    }

    /// Collect all labels for a specific node from the inverted labels index.
    pub fn labels_for_node(&self, node_idx: NodeIDX) -> BTreeMap<&str, &BTreeSet<LabelValue>> {
        self.labels
            .iter()
            .filter_map(|(label_name, node_map)| {
                node_map
                    .get(&node_idx)
                    .map(|values| (label_name.as_str(), values))
            })
            .collect()
    }

    /// Collect all properties for a specific node from the inverted properties index.
    pub fn properties_for_node(&self, node_idx: NodeIDX) -> BTreeMap<&str, &str> {
        self.properties
            .iter()
            .filter_map(|(prop_name, node_map)| {
                node_map
                    .get(&node_idx)
                    .map(|value| (prop_name.as_str(), value.as_str()))
            })
            .collect()
    }
}

impl ArrayGraphSerializable {
    /// Returns an iterator over all valid node indices in this graph.
    pub fn node_idx_iter(&self) -> impl Iterator<Item = NodeIDX> {
        (0..self.node_names_ordered.combined_nodes_len()).map(NodeIDX::from)
    }

    /// Converts this serializable representation back into an `ArrayGraph`.
    ///
    /// Rebuilds the full [`OffsetGraph`] by merging the CSR-encoded directed edges
    /// with the tagged and dynamic edges. Each phase is wrapped in an `#l3` task
    /// for structured tracing.
    ///
    /// The algorithm avoids intermediate copies by:
    /// 1. Counting edges per node in parallel (no allocations)
    /// 2. Computing offsets via prefix sum
    /// 3. Writing all edges directly into pre-allocated arrays in a single parallel pass
    pub fn into_array_graph(self, task: &ll::Task) -> Result<ArrayGraph> {
        let edges_ser = self.edges;
        let node_names_ordered = self.node_names_ordered;
        let node_metadata = self.node_metadata;
        let traversal_config = self.traversal_config;
        let graph_settings = self.graph_settings;
        let entry_points = self.entry_points;
        let properties = self.properties;

        task.spawn_sync("into_array_graph", |task| {
            let node_count = node_names_ordered.combined_nodes_len();

            // Count edges per node in parallel (just arithmetic, no allocations).
            let edge_counts: Vec<usize> = task.spawn_sync("count_edges #l3", |_| {
                Ok((0..node_count)
                    .into_par_iter()
                    .map(|i| {
                        let node_idx = NodeIDX::from(i);
                        let dir = edges_ser.directed_offsets[i + 1] - edges_ser.directed_offsets[i];
                        let tagged = edges_ser
                            .tagged
                            .get(&node_idx)
                            .map_or(0, |t| t.values().map(|pts| pts.len()).sum());
                        let dynamic = edges_ser.dynamic.get(&node_idx).map_or(0, |tm| {
                            tm.values()
                                .flat_map(|em| em.values())
                                .flat_map(|de| de.branches.values())
                                .map(|idxs| idxs.len())
                                .sum()
                        });
                        dir + tagged + dynamic
                    })
                    .collect())
            })?;

            // Prefix sum → edge_offsets (sequential, but just 30M additions).
            let total_edges: usize = edge_counts.iter().sum();
            let mut edge_offsets = Vec::with_capacity(node_count + 1);
            edge_offsets.push(0usize);
            for &count in &edge_counts {
                edge_offsets.push(edge_offsets[edge_offsets.len() - 1] + count);
            }
            drop(edge_counts);

            // Pre-allocate final arrays and fill in a single parallel pass.
            // Each thread writes to a non-overlapping range determined by edge_offsets.
            let edges_forward = task.spawn_sync("fill_edges #l3", |_| {
                let mut all_edges: Vec<Edge> = Vec::with_capacity(total_edges);
                let mut all_metadata: Vec<NonDirectedEdgeMetadata> =
                    Vec::with_capacity(total_edges);

                // SAFETY: set_len is sound because every element is written exactly once
                // in the parallel loop below via ptr::write, and Edge/NonDirectedEdgeMetadata
                // don't implement Drop (no double-free on uninitialized memory).
                unsafe {
                    all_edges.set_len(total_edges);
                    all_metadata.set_len(total_edges);
                }

                let edges_ptr = all_edges.as_mut_ptr() as usize;
                let meta_ptr = all_metadata.as_mut_ptr() as usize;

                (0..node_count).into_par_iter().for_each(|i| {
                    let node_idx = NodeIDX::from(i);
                    let base = edge_offsets[i];
                    let mut pos = 0;

                    let ep = edges_ptr as *mut Edge;
                    let mp = meta_ptr as *mut NonDirectedEdgeMetadata;

                    // Directed edges — read directly from serialized CSR.
                    let dir_start = edges_ser.directed_offsets[i];
                    let dir_end = edges_ser.directed_offsets[i + 1];
                    for &points_to in &edges_ser.directed[dir_start..dir_end] {
                        unsafe {
                            std::ptr::write(ep.add(base + pos), Edge::new(points_to));
                            std::ptr::write(mp.add(base + pos), NonDirectedEdgeMetadata::Directed);
                        }
                        pos += 1;
                    }

                    // Tagged edges.
                    if let Some(tagged) = edges_ser.tagged.get(&node_idx) {
                        for (tag, points_to_set) in tagged {
                            for &points_to in points_to_set {
                                unsafe {
                                    std::ptr::write(
                                        ep.add(base + pos),
                                        Edge::new_tagged(points_to),
                                    );
                                    std::ptr::write(
                                        mp.add(base + pos),
                                        NonDirectedEdgeMetadata::Tagged { tag: tag.clone() },
                                    );
                                }
                                pos += 1;
                            }
                        }
                    }

                    // Dynamic edges.
                    if let Some(type_map) = edges_ser.dynamic.get(&node_idx) {
                        for (type_key, edge_map) in type_map {
                            for (edge_name, dynamic_edge) in edge_map {
                                for (branch, node_idxs) in &dynamic_edge.branches {
                                    for &points_to in node_idxs {
                                        unsafe {
                                            std::ptr::write(
                                                ep.add(base + pos),
                                                Edge::new_dynamic(points_to),
                                            );
                                            std::ptr::write(
                                                mp.add(base + pos),
                                                NonDirectedEdgeMetadata::Dynamic {
                                                    type_key: type_key.clone(),
                                                    edge_name: edge_name.clone(),
                                                    branch: branch.clone(),
                                                },
                                            );
                                        }
                                        pos += 1;
                                    }
                                }
                            }
                        }
                    }
                });

                Ok(OffsetGraph {
                    edges: all_edges,
                    edge_offsets,
                    non_directed_edges_metadata: all_metadata,
                })
            })?;

            let node_flags = vec![NodeFlags::empty(); node_names_ordered.combined_nodes_len()];

            let tiers = traversal_config
                .as_ref()
                .map_or_else(Default::default, |config| config.get_tiers());

            let derived_state = ArrayGraphDerivedState::new();
            let nodes = ArrayGraphNodesForGraphSide::new_left_only(node_names_ordered);

            Ok(ArrayGraph {
                nodes,
                edges_forward,
                derived_state,
                edges_tagged: edges_ser.tagged,
                edges_dynamic: edges_ser.dynamic,
                node_metrics: node_metadata.metrics,
                node_labels: node_metadata.labels,
                node_properties: node_metadata.properties,
                node_flags,
                state: ArrayGraphState {
                    traversal_config: traversal_config.clone(),
                    indexed_messages: Default::default(),
                    tiers,
                },
                graph_settings,
                entry_points,
                properties,
            })
        })
    }

    /// Serializes this graph to a JSON string.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize ArrayGraphSerializable to JSON")
    }

    /// Deserializes a graph from a JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize ArrayGraphSerializable from JSON")
    }

    /// Deserializes a graph from raw JSON bytes.
    pub fn from_json_bytes(json: &[u8]) -> Result<Self> {
        serde_json::from_slice(json)
            .context("Failed to deserialize ArrayGraphSerializable from JSON bytes")
    }

    /// Packs this graph into compressed, chunked blobs for storage/transport.
    ///
    /// Each field is serialized, compressed, and chunked in parallel via rayon
    /// (degrades to sequential on WASM).
    pub fn pack(
        &self,
        config: &ArrayGraphSerializablePackageConfig,
        task: &ll::Task,
    ) -> Result<ArrayGraphSerializablePackage> {
        package::pack(self, config, task).context("Failed to pack graph")
    }

    pub fn unpack(
        package: &ArrayGraphSerializablePackage,
        task: &ll::Task,
    ) -> Result<ArrayGraphSerializable> {
        package::unpack(package, task).context("Failed to unpack graph")
    }

    /// Remaps node indices throughout the entire graph (names, edges, metadata)
    /// according to the given [`RemapContext`].
    pub fn remap(self, ctx: &RemapContext) -> Result<Self> {
        Ok(ArrayGraphSerializable {
            node_names_ordered: Arc::new(remap_node_names_ordered(&self.node_names_ordered, ctx)?),
            edges: self.edges.remap(ctx)?,
            node_metadata: self.node_metadata.remap(ctx)?,
            graph_settings: self.graph_settings,
            traversal_config: self.traversal_config,
            entry_points: self.entry_points,
            properties: self.properties,
        })
    }
}

/// Converts an [`ArrayGraph`] into its serializable form.
///
/// Directed edges are extracted into a flat CSR layout, filtering out tagged
/// and dynamic edges (which are stored separately) to avoid duplication.
impl From<ArrayGraph> for ArrayGraphSerializable {
    fn from(graph: ArrayGraph) -> Self {
        let mut directed_edges = vec![];
        let mut directed_edge_offsets = vec![0];

        for node_idx in graph.edges_forward.node_idx_iter() {
            for edge in graph.edges_forward.edges(node_idx) {
                // In ArrayGraph the offset graph contains all edges, including tagged and dynamic ones.
                // This data is duplicated and dynamic/tagged edges are also stored on the graph.
                // When we serialize we don't want to include the extra data for efficiency, so we
                // will filter them out. When we deserialize we will be able to reconstruct the
                // original offset graph because we still retain the tagged and dynamic edges
                // in the graph.
                if !edge.is_tagged_or_dynamic() {
                    directed_edges.push(edge.points_to);
                }
            }
            directed_edge_offsets.push(directed_edges.len());
        }

        ArrayGraphSerializable {
            node_names_ordered: graph.nodes.node_names,
            edges: ArrayGraphSerializableEdges {
                directed: directed_edges,
                directed_offsets: directed_edge_offsets,
                tagged: graph.edges_tagged,
                dynamic: graph.edges_dynamic,
            },
            node_metadata: ArrayGraphSerializableNodeMetadata {
                metrics: graph.node_metrics,
                labels: graph.node_labels,
                properties: graph.node_properties,
            },
            graph_settings: graph.graph_settings,
            traversal_config: graph.state.traversal_config,
            entry_points: graph.entry_points,
            properties: graph.properties,
        }
    }
}

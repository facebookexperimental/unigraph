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
use crate::types::MetricName;
use crate::types::NodeIDX;
use crate::types::NodeName;
use crate::types::Tag;
use crate::types::TagSetName;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_nodes::ArrayGraphNodesForGraphSide;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;
use crate::types::array_graph::offset_graph::OffsetGraph;

/// A serializable representation of an array graph, which can be used for
/// storing or transmitting the graph structure.
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen)]
pub struct ArrayGraphSerializable {
    pub node_names_ordered: Arc<ArrayGraphNodes>,
    pub edges: ArrayGraphSerializableEdges,
    pub node_metadata: ArrayGraphSerializableNodeMetadata,

    pub graph_settings: Option<GraphSettings>,
    pub traversal_config: Option<TraversalConfig>,

    /// If present, these graph will use these entrypoints instead
    /// of automatically determining them.
    pub entry_points: Option<BTreeSet<NodeName>>,
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
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen)]
pub struct ArrayGraphSerializableEdges {
    /// Flat list of directed-edge target node indices.
    pub directed: Vec<NodeIDX>,
    /// CSR offsets into `directed` — `directed[directed_offsets[i]..directed_offsets[i+1]]`
    /// gives the targets for source node `i`.
    pub directed_offsets: Vec<usize>,
    /// Tagged edges: source node → tag → set of target nodes.
    pub tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    /// Dynamic edges with runtime-defined branches and properties.
    pub dynamic: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
}

impl ArrayGraphSerializableEdges {
    /// Remaps all node indices in this edge set according to the given context.
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_edges(self, ctx).context("Failed to remap ArrayGraphSerializableEdges")
    }
}

/// Serializable per-node metadata: numeric metrics and categorical tag sets.
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen)]
pub struct ArrayGraphSerializableNodeMetadata {
    /// Named metrics — each entry maps a metric name to a `Vec<f32>` with one
    /// value per node (indexed by [`NodeIDX`]).
    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    /// Per-node tag sets — maps a node index to its named tag sets, where each
    /// tag set contains a collection of string tags.
    pub tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>>,
}

impl ArrayGraphSerializableNodeMetadata {
    /// Remaps all node indices in the metadata according to the given context.
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_node_metadata(self, ctx).context("Failed to remap ArrayGraphSerializableNodeMetadata")
    }
}

impl ArrayGraphSerializable {
    /// Returns an iterator over all valid node indices in this graph.
    pub fn node_idx_iter(&self) -> impl Iterator<Item = NodeIDX> {
        (0..self.node_names_ordered.combined_nodes_len()).map(NodeIDX::from)
    }

    /// Converts this serializable representation back into an `ArrayGraph`.
    pub fn into_array_graph(self) -> ArrayGraph {
        self.into()
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

    /// NOTE: this is a serial operation that is performed on a single thread.
    /// it works on WASM but it will be much much slower in native than it could be.
    /// You should use the parallelized version in `unigraph_server_utils` crate instead.
    pub fn pack(
        &self,
        config: &ArrayGraphSerializablePackageConfig,
    ) -> Result<ArrayGraphSerializablePackage> {
        package::pack(self, config).context("Failed to pack graph")
    }

    pub fn unpack(package: &ArrayGraphSerializablePackage) -> Result<ArrayGraphSerializable> {
        package::unpack(package).context("Failed to unpack graph")
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
                metrics: graph.metrics,
                tag_sets: graph.tag_sets,
            },
            graph_settings: graph.graph_settings,
            traversal_config: graph.state.traversal_config,
            entry_points: graph.entry_points,
        }
    }
}

/// Reconstructs an [`ArrayGraph`] from its serializable form.
///
/// Rebuilds the full [`OffsetGraph`] by merging the CSR-encoded directed edges
/// with the tagged and dynamic edges. Derived state (reverse edges, etc.) is
/// recomputed from the reconstructed forward edges.
impl From<ArrayGraphSerializable> for ArrayGraph {
    fn from(serializable: ArrayGraphSerializable) -> Self {
        // make an offset graph containing only directed edges so we
        // can use its functions to build the ArrayGraph
        let directed_only_offset_graph = OffsetGraph {
            edges: serializable
                .edges
                .directed
                .iter()
                .map(|points_to| Edge::new(*points_to))
                .collect(),
            edge_offsets: serializable.edges.directed_offsets,
            non_directed_edges_metadata: vec![
                NonDirectedEdgeMetadata::Directed;
                serializable.edges.directed.len()
            ],
        };

        let mut edges_forward = OffsetGraph {
            edges: vec![],
            edge_offsets: vec![0],
            non_directed_edges_metadata: vec![],
        };

        for node_idx in directed_only_offset_graph.node_idx_iter() {
            for edge in directed_only_offset_graph.edges(node_idx) {
                edges_forward.edges.push(Edge::new(edge.points_to));
                edges_forward
                    .non_directed_edges_metadata
                    .push(NonDirectedEdgeMetadata::Directed);
            }

            if let Some(tagged) = serializable.edges.tagged.get(&node_idx) {
                for (tag, points_to_set) in tagged {
                    for &points_to in points_to_set {
                        edges_forward.edges.push(Edge::new_tagged(points_to));
                        edges_forward
                            .non_directed_edges_metadata
                            .push(NonDirectedEdgeMetadata::Tagged { tag: tag.clone() });
                    }
                }
            }

            if let Some(dynamic) = serializable.edges.dynamic.get(&node_idx) {
                for dynamic_edge in dynamic {
                    for (branch, node_idxs) in &dynamic_edge.branches {
                        for &points_to in node_idxs {
                            edges_forward.edges.push(Edge::new_dynamic(points_to));
                            edges_forward.non_directed_edges_metadata.push(
                                NonDirectedEdgeMetadata::Dynamic {
                                    properties: dynamic_edge.properties.clone(),
                                    branch: branch.clone(),
                                },
                            );
                        }
                    }
                }
            }
            edges_forward.edge_offsets.push(edges_forward.edges.len());
        }

        // Node flags are initialized on traversal config application, so we'll just create an empty vector
        let node_flags =
            vec![NodeFlags::empty(); serializable.node_names_ordered.combined_nodes_len()];

        let tiers = serializable
            .traversal_config
            .as_ref()
            .map_or_else(Default::default, |config| config.get_tiers());

        let derived_state = ArrayGraphDerivedState::from_forward_edges(&edges_forward);

        let nodes = ArrayGraphNodesForGraphSide::new_left_only(serializable.node_names_ordered);

        ArrayGraph {
            nodes,
            edges_forward,
            derived_state,
            edges_tagged: serializable.edges.tagged,
            edges_dynamic: serializable.edges.dynamic,
            metrics: serializable.node_metadata.metrics,
            tag_sets: serializable.node_metadata.tag_sets,
            node_flags,
            state: ArrayGraphState {
                traversal_config: serializable.traversal_config.clone(),
                indexed_messages: Default::default(),
                tiers,
            },
            graph_settings: serializable.graph_settings,
            entry_points: serializable.entry_points,
        }
    }
}

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use super::ArrayGraph;
use super::ArrayGraphDynamicEdge;
use super::NodeFlags;
use super::node_names_ordered::NodeNamesOrdered;
use super::offset_graph::Edge;
use super::offset_graph::NonDirectedEdgeMetadata;
use super::offset_graph::OffsetGraph;
use crate::TraversalConfig;
use crate::array_graph_settings::ArrayGraphSettings;
use crate::remap_utils::RemapContext;
use crate::remap_utils::remap_edges;
use crate::remap_utils::remap_node_metadata;
use crate::types::MetricName;
use crate::types::NodeIDX;
use crate::types::Tag;
use crate::types::TagSetName;

/// A serializable representation of an array graph, which can be used for
/// storing or transmitting the graph structure.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializable {
    pub node_names_ordered: NodeNamesOrdered,
    pub edges: ArrayGraphSerializableEdges,
    pub node_metadata: ArrayGraphSerializableNodeMetadata,

    pub array_graph_settings: Option<ArrayGraphSettings>,
    pub traversal_config: Option<TraversalConfig>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializableEdges {
    pub directed: Vec<NodeIDX>,
    pub directed_offsets: Vec<usize>,
    pub tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    pub dynamic: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
}

impl ArrayGraphSerializableEdges {
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_edges(self, ctx).context("Failed to remap ArrayGraphSerializableEdges")
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializableNodeMetadata {
    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>>,
}

impl ArrayGraphSerializableNodeMetadata {
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_node_metadata(self, ctx).context("Failed to remap ArrayGraphSerializableNodeMetadata")
    }
}

impl ArrayGraphSerializable {
    pub fn node_idx_iter(&self) -> impl Iterator<Item = NodeIDX> {
        (0..self.node_names_ordered.nodes_len()).map(NodeIDX::from)
    }

    /// Converts this serializable representation back into an `ArrayGraph`.
    pub fn to_array_graph(self) -> ArrayGraph {
        self.into()
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize ArrayGraphSerializable to JSON")
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize ArrayGraphSerializable from JSON")
    }
    pub fn from_json_bytes(json: &[u8]) -> Result<Self> {
        serde_json::from_slice(json)
            .context("Failed to deserialize ArrayGraphSerializable from JSON bytes")
    }
}

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
            node_names_ordered: graph.node_names_ordered,
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
            array_graph_settings: None,
            traversal_config: None,
        }
    }
}

impl From<ArrayGraphSerializable> for ArrayGraph {
    fn from(mut serializable: ArrayGraphSerializable) -> Self {
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

            if let Some(tagged) = serializable.edges.tagged.remove(&node_idx) {
                for (tag, points_to_set) in tagged {
                    for points_to in points_to_set {
                        edges_forward.edges.push(Edge::new_tagged(points_to));
                        edges_forward
                            .non_directed_edges_metadata
                            .push(NonDirectedEdgeMetadata::Tagged { tag: tag.clone() });
                    }
                }
            }

            if let Some(dynamic) = serializable.edges.dynamic.remove(&node_idx) {
                for dynamic_edge in dynamic {
                    for (branch, node_idxs) in dynamic_edge.branches {
                        for points_to in node_idxs {
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

        let edges_reverse = edges_forward.reverse();

        // Node flags are initialized on traversal config application, so we'll just create an empty vector
        let node_flags = vec![NodeFlags::empty(); serializable.node_names_ordered.nodes_len()];

        ArrayGraph {
            node_names_ordered: serializable.node_names_ordered,
            edges_forward,
            edges_reverse,
            edges_tagged: serializable.edges.tagged,
            edges_dynamic: serializable.edges.dynamic,
            metrics: serializable.node_metadata.metrics,
            tag_sets: serializable.node_metadata.tag_sets,
            node_flags,
            traversal_config: None,
        }
    }
}

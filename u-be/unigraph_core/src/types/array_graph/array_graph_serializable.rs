use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use base64::Engine;

use super::ArrayGraph;
use super::ArrayGraphDynamicEdge;
use super::NodeFlags;
use super::node_names_ordered::NodeNamesOrdered;
use super::offset_graph::Edge;
use super::offset_graph::NonDirectedEdgeMetadata;
use super::offset_graph::OffsetGraph;
use crate::types::MetricName;
use crate::types::NodeIDX;
use crate::types::Tag;
use crate::types::TagSetName;

/// A serializable representation of an array graph, which can be used for
/// storing or transmitting the graph structure.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializable {
    pub node_names_ordered: NodeNamesOrdered,
    pub directed_edges: Vec<NodeIDX>,
    pub directed_edge_offsets: Vec<usize>,
    pub edges_tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    pub edges_dynamic: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,

    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>>,
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

    pub fn to_json_zstd(&self) -> Result<Vec<u8>> {
        let json = self.to_json()?;
        let compressed =
            zstd::encode_all(json.as_bytes(), 18).context("Failed to compress JSON")?;
        Ok(compressed)
    }

    pub fn to_json_zstd_base64(&self) -> Result<String> {
        let compressed = self.to_json_zstd()?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&compressed))
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize ArrayGraphSerializable from JSON")
    }

    pub fn from_json_zstd(compressed: &[u8]) -> Result<Self> {
        let decompressed = zstd::decode_all(compressed).context("Failed to decompress JSON")?;
        serde_json::from_slice(&decompressed)
            .context("Failed to deserialize ArrayGraphSerializable from decompressed JSON")
    }

    pub fn from_json_zstd_base64(base64_str: &str) -> Result<Self> {
        let compressed = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .context("Failed to decode base64 string")?;
        Self::from_json_zstd(&compressed)
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
            directed_edges,
            directed_edge_offsets,

            edges_tagged: graph.edges_tagged,
            edges_dynamic: graph.edges_dynamic,

            metrics: graph.metrics,
            tag_sets: graph.tag_sets,
        }
    }
}

impl From<ArrayGraphSerializable> for ArrayGraph {
    fn from(mut serializable: ArrayGraphSerializable) -> Self {
        // make an offset graph containing only directed edges so we
        // can use its functions to build the ArrayGraph
        let directed_only_offset_graph = OffsetGraph {
            edges: serializable
                .directed_edges
                .iter()
                .map(|points_to| Edge::new(*points_to))
                .collect(),
            edge_offsets: serializable.directed_edge_offsets,
            non_directed_edges_metadata: vec![
                NonDirectedEdgeMetadata::Directed;
                serializable.directed_edges.len()
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

            if let Some(tagged) = serializable.edges_tagged.remove(&node_idx) {
                for (tag, points_to_set) in tagged {
                    for points_to in points_to_set {
                        edges_forward.edges.push(Edge::new_tagged(points_to));
                        edges_forward
                            .non_directed_edges_metadata
                            .push(NonDirectedEdgeMetadata::Tagged { tag: tag.clone() });
                    }
                }
            }

            if let Some(dynamic) = serializable.edges_dynamic.remove(&node_idx) {
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
            edges_tagged: serializable.edges_tagged,
            edges_dynamic: serializable.edges_dynamic,
            metrics: serializable.metrics,
            tag_sets: serializable.tag_sets,
            node_flags,
        }
    }
}

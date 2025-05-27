// Copyright (c) Meta Platforms, Inc. and affiliates.

mod offset_graph_traversal;

use std::collections::BTreeMap;
use std::collections::HashMap;

use offset_graph_traversal::EdgesIterMut;
use offset_graph_traversal::OffsetGraphDFS;

use super::Arrow;
use crate::types::NodeIDX;

pub struct OffsetGraph {
    edge_offsets: Vec<usize>,
    edges: Vec<Edge>,
    non_directed_edges_metadata: Vec<NonDirectedEdgeMetadata>,
}

/// Metadata for non-directed edges in the graph that contains
/// flattened, per edge data for easier access when we need to
/// construct error, or reverse graphs, etc.
#[derive(Clone, Debug)]
pub enum NonDirectedEdgeMetadata {
    Directed,
    Tagged {
        tag: String,
    },
    Dynamic {
        properties: BTreeMap<String, String>,
        branch: String,
    },
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct EdgeFlags: u32 {
        const LEFT_IS_TAGGED =   0b0000_0001_0000_0000;
        const LEFT_IS_DYNAMIC =  0b0000_0010_0000_0000;
        const LEFT_EXCLUDED =    0b0000_0100_0000_0000;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Edge {
    pub points_to: NodeIDX,
    pub flags: EdgeFlags,
}

impl Edge {
    /// Simple edge that points to another node but has no flags set
    pub fn new(points_to: NodeIDX) -> Self {
        Edge {
            points_to,
            flags: EdgeFlags::empty(),
        }
    }

    pub fn new_with_flags(points_to: NodeIDX, flags: EdgeFlags) -> Self {
        Edge { points_to, flags }
    }
}

impl EdgeFlags {
    pub fn to_binary_string(self) -> String {
        let binary = format!("{:016b}", self.bits());
        let mut result = String::with_capacity(19); // 16 digits + 3 separators
        for (i, c) in binary.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                result.push('_');
            }
            result.push(c);
        }
        result
    }
}

pub struct OffsetGraphBuilder {
    edges: Vec<Edge>,
    edge_offsets: Vec<usize>,
    non_directed_edges_metadata: Vec<NonDirectedEdgeMetadata>,
}

impl OffsetGraphBuilder {
    pub fn new() -> Self {
        OffsetGraphBuilder {
            edges: Vec::new(),
            edge_offsets: vec![0],
            non_directed_edges_metadata: Vec::new(),
        }
    }

    /// Push the next node to the graph. This node IDX will be the
    /// next IDX of the offsets vector
    pub fn push_node<I: IntoIterator<Item = Arrow>>(&mut self, arrows: I) {
        for arrow in arrows {
            if let Some(tag) = arrow.tag {
                self.edges.push(Edge::new_with_flags(
                    arrow.points_to,
                    EdgeFlags::LEFT_IS_TAGGED,
                ));
                self.non_directed_edges_metadata
                    .push(NonDirectedEdgeMetadata::Tagged { tag });
            } else if let Some(branch) = arrow.branch {
                self.edges.push(Edge::new_with_flags(
                    arrow.points_to,
                    EdgeFlags::LEFT_IS_DYNAMIC,
                ));
                self.non_directed_edges_metadata
                    .push(NonDirectedEdgeMetadata::Dynamic {
                        properties: arrow.properties.unwrap_or_default(),
                        branch,
                    });
            } else {
                self.edges.push(Edge::new(arrow.points_to));
                self.non_directed_edges_metadata
                    .push(NonDirectedEdgeMetadata::Directed);
            }
        }

        self.edge_offsets.push(self.edges.len());
    }

    pub fn build(self) -> OffsetGraph {
        OffsetGraph {
            edge_offsets: self.edge_offsets,
            edges: self.edges,
            non_directed_edges_metadata: self.non_directed_edges_metadata,
        }
    }
}

impl OffsetGraph {
    // how many nodes are there in the graph
    pub fn node_count(&self) -> usize {
        // subract one for that initial 0 that we push in the beginning.
        self.edge_offsets.len() - 1
    }

    pub fn edges_len(&self) -> usize {
        self.edges.len()
    }

    pub fn node_idx_iter(&self) -> impl Iterator<Item = NodeIDX> {
        (0..self.node_count()).map(NodeIDX::from)
    }

    pub fn edges(&self, node_idx: NodeIDX) -> &[Edge] {
        let start = self.edge_offsets[node_idx];
        let end = self.edge_offsets[node_idx + 1];
        &self.edges[start..end]
    }

    pub fn iter_edges_mut(&mut self) -> EdgesIterMut {
        EdgesIterMut::new(self)
    }

    pub fn edges_with_metadata(
        &self,
        node_idx: NodeIDX,
    ) -> impl Iterator<Item = (Edge, &NonDirectedEdgeMetadata)> {
        let start = self.edge_offsets[node_idx];
        let end = self.edge_offsets[node_idx + 1];
        (start..end).map(|edge_idx| {
            (
                self.edges[edge_idx],
                &self.non_directed_edges_metadata[edge_idx],
            )
        })
    }

    pub fn reverse(&self) -> OffsetGraph {
        // make an offset graph of children to parents
        let mut child_to_parent: HashMap<NodeIDX, Vec<(Edge, NonDirectedEdgeMetadata)>> =
            HashMap::new();
        for node_id in self.node_idx_iter() {
            for (child, metadata) in self.edges_with_metadata(node_id) {
                child_to_parent
                    .entry(child.points_to)
                    .or_insert_with(Vec::new)
                    .push((
                        Edge {
                            points_to: node_id,
                            flags: child.flags,
                        },
                        metadata.clone(),
                    ));
            }
        }

        let mut reverse_graph = OffsetGraph {
            edges: Vec::new(),
            edge_offsets: vec![0],
            non_directed_edges_metadata: Vec::new(),
        };

        for node_id in self.node_idx_iter() {
            let parents = child_to_parent.remove(&node_id);
            for (edge, metadata) in parents.into_iter().flatten() {
                reverse_graph.edges.push(edge);
                reverse_graph.non_directed_edges_metadata.push(metadata);
            }
            reverse_graph.edge_offsets.push(reverse_graph.edges.len());
        }
        reverse_graph
    }

    pub fn dfs(&self, roots: &[NodeIDX]) -> OffsetGraphDFS {
        OffsetGraphDFS::new(self, roots)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::*;

    use super::*;

    #[test]
    fn edge_test() {
        assert_equal!(std::mem::size_of::<Edge>(), 8);
    }

    #[test]
    fn test_edge_flags() -> Result<()> {
        let edge = Edge::new_with_flags(NodeIDX(1), EdgeFlags::LEFT_IS_TAGGED);
        assert_equal!(edge.flags.to_binary_string(), "0000_0001_0000_0000");
        assert_equal!(edge.flags.contains(EdgeFlags::LEFT_IS_TAGGED), true);
        assert_equal!(edge.flags.intersects(EdgeFlags::LEFT_IS_TAGGED), true);
        assert_equal!(edge.flags.intersects(EdgeFlags::LEFT_IS_DYNAMIC), false);
        assert_equal!(
            edge.flags
                .intersects(EdgeFlags::LEFT_IS_TAGGED | EdgeFlags::LEFT_IS_DYNAMIC),
            true
        );

        assert_equal!(edge.flags.to_binary_string(), "0000_0001_0000_0000");

        let edge = Edge::new_with_flags(NodeIDX(1), EdgeFlags::LEFT_IS_DYNAMIC);
        assert_equal!(edge.flags.contains(EdgeFlags::LEFT_IS_DYNAMIC), true);

        assert_equal!(edge.flags.to_binary_string(), "0000_0010_0000_0000");

        Ok(())
    }
}

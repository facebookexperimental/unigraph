// Copyright (c) Meta Platforms, Inc. and affiliates.

pub(super) mod lengauer_tarjan_dominator_tree;
mod offset_graph_traversal;

use std::collections::BTreeMap;
use std::collections::HashMap;

use offset_graph_traversal::EdgesIterMut;
use offset_graph_traversal::OffsetGraphDFSConfigured;

use super::Arrow;
use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::offset_graph_traversal::EdgesIter;
use crate::types::array_graph::offset_graph::offset_graph_traversal::OffsetGraphDFSUnconfigured;

pub struct OffsetGraph {
    pub(super) edges: Vec<Edge>,
    pub(super) edge_offsets: Vec<usize>,
    pub(super) non_directed_edges_metadata: Vec<NonDirectedEdgeMetadata>,
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
        const IS_TAGGED =   0b0000_0001;
        const IS_DYNAMIC =  0b0000_0010;
        const EXCLUDED =    0b0000_0100;
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

    pub fn new_tagged(points_to: NodeIDX) -> Self {
        Edge {
            points_to,
            flags: EdgeFlags::IS_TAGGED,
        }
    }

    pub fn new_dynamic(points_to: NodeIDX) -> Self {
        Edge {
            points_to,
            flags: EdgeFlags::IS_DYNAMIC,
        }
    }

    pub fn is_tagged_or_dynamic(&self) -> bool {
        self.flags
            .intersects(EdgeFlags::IS_TAGGED | EdgeFlags::IS_DYNAMIC)
    }

    #[inline(always)]
    pub fn is_excluded(&self) -> bool {
        self.flags.contains(EdgeFlags::EXCLUDED)
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

    #[inline(always)]
    pub fn is_excluded(&self) -> bool {
        self.contains(EdgeFlags::EXCLUDED)
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
                self.edges
                    .push(Edge::new_with_flags(arrow.points_to, EdgeFlags::IS_TAGGED));
                self.non_directed_edges_metadata
                    .push(NonDirectedEdgeMetadata::Tagged { tag });
            } else if let Some(branch) = arrow.branch {
                self.edges
                    .push(Edge::new_with_flags(arrow.points_to, EdgeFlags::IS_DYNAMIC));
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

    /// is this correct?? should be unconfigured?? --aaron 2025-06-13
    pub fn edges_len_for_node_configured(&self, node_idx: NodeIDX) -> usize {
        let start = self.edge_offsets[node_idx];
        let end = self.edge_offsets[node_idx + 1];
        end - start
    }

    pub fn node_idx_iter(&self) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.node_count()).map(NodeIDX::from)
    }

    pub fn edges(&self, node_idx: NodeIDX) -> &[Edge] {
        let start = self.edge_offsets[node_idx];
        let end = self.edge_offsets[node_idx + 1];
        &self.edges[start..end]
    }

    pub fn edges_configured(&self, node_idx: NodeIDX) -> impl Iterator<Item = Edge> {
        let start = self.edge_offsets[node_idx];
        let end = self.edge_offsets[node_idx + 1];
        self.edges[start..end]
            .iter()
            .copied()
            .filter(|edge| !edge.is_excluded())
    }

    pub fn iter_edges(&self) -> impl Iterator<Item = (NodeIDX, Edge, &NonDirectedEdgeMetadata)> {
        EdgesIter::new(self)
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
                child_to_parent.entry(child.points_to).or_default().push((
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

    pub fn dfs_configured(&self, roots: &[NodeIDX]) -> OffsetGraphDFSConfigured {
        OffsetGraphDFSConfigured::new(self, roots)
    }

    pub fn dfs_unconfigured(&self, roots: &[NodeIDX]) -> OffsetGraphDFSUnconfigured {
        OffsetGraphDFSUnconfigured::new(self, roots)
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
        let edge = Edge::new_with_flags(NodeIDX(1), EdgeFlags::IS_TAGGED);
        assert_equal!(edge.flags.to_binary_string(), "0000_0000_0000_0001");
        assert_equal!(edge.flags.contains(EdgeFlags::IS_TAGGED), true);
        assert_equal!(edge.flags.intersects(EdgeFlags::IS_TAGGED), true);
        assert_equal!(edge.flags.intersects(EdgeFlags::IS_DYNAMIC), false);
        assert_equal!(
            edge.flags
                .intersects(EdgeFlags::IS_TAGGED | EdgeFlags::IS_DYNAMIC),
            true
        );

        assert_equal!(edge.flags.to_binary_string(), "0000_0000_0000_0001");

        let edge = Edge::new_with_flags(NodeIDX(1), EdgeFlags::IS_DYNAMIC);
        assert_equal!(edge.flags.contains(EdgeFlags::IS_DYNAMIC), true);

        assert_equal!(edge.flags.to_binary_string(), "0000_0000_0000_0010");

        Ok(())
    }
}

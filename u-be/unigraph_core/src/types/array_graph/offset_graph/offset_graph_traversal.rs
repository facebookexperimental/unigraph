// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashSet;

use super::Edge;
use super::EdgeFlags;
use super::NonDirectedEdgeMetadata;
use super::OffsetGraph;
use crate::types::NodeIDX;

/// A depth-first search (DFS) iterator for traversing an `OffsetGraph`
/// with that WILL use traversal config to determinde which edges to follow
/// and which to skip.
pub struct OffsetGraphDFSConfigured<'a> {
    offset_graph: &'a OffsetGraph,
    stack: Vec<NodeIDX>,
    visited: HashSet<NodeIDX>,
}

impl<'a> OffsetGraphDFSConfigured<'a> {
    pub fn new(graph: &'a OffsetGraph, roots: &[NodeIDX]) -> Self {
        OffsetGraphDFSConfigured {
            offset_graph: graph,
            stack: roots.to_vec(),
            // Optimization opportunity: use a Vec<bool> instead of HashSet
            // for instant memory access in exchange for a using more memory
            // We can also split the DFS for most cases into populating Visited
            // VEC first and then doing a single loop over that vec to get
            // the NodeIDXs of visited and do operations in bacth that can optimize
            // for SIMD.
            visited: HashSet::new(),
        }
    }
}

impl<'a> Iterator for OffsetGraphDFSConfigured<'a> {
    type Item = NodeIDX;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node_idx) = self.stack.pop() {
            if !self.visited.contains(&node_idx) {
                self.visited.insert(node_idx);
                self.stack
                    .extend(self.offset_graph.edges(node_idx).iter().filter_map(|e| {
                        (!e.flags.contains(EdgeFlags::EXCLUDED)).then_some(e.points_to)
                    }));
                return Some(node_idx);
            }
        }
        None
    }
}

/// A depth-first search (DFS) iterator for traversing an `OffsetGraph`
/// that WILL NOT use traversal config to determinde which edges to follow.
/// It will traverse ALL possible edges regardless of their exclusion flags.
pub struct OffsetGraphDFSUnconfigured<'a> {
    offset_graph: &'a OffsetGraph,
    stack: Vec<NodeIDX>,
    visited: HashSet<NodeIDX>,
}

impl<'a> OffsetGraphDFSUnconfigured<'a> {
    pub fn new(graph: &'a OffsetGraph, roots: &[NodeIDX]) -> Self {
        OffsetGraphDFSUnconfigured {
            offset_graph: graph,
            stack: roots.to_vec(),
            // Optimization opportunity: use a Vec<bool> instead of HashSet
            // for instant memory access in exchange for a using more memory
            // We can also split the DFS for most cases into populating Visited
            // VEC first and then doing a single loop over that vec to get
            // the NodeIDXs of visited and do operations in bacth that can optimize
            // for SIMD.
            visited: HashSet::new(),
        }
    }
}

impl<'a> Iterator for OffsetGraphDFSUnconfigured<'a> {
    type Item = NodeIDX;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node_idx) = self.stack.pop() {
            if !self.visited.contains(&node_idx) {
                self.visited.insert(node_idx);
                self.stack.extend(
                    self.offset_graph
                        .edges(node_idx)
                        .iter()
                        .map(|e| e.points_to),
                );
                return Some(node_idx);
            }
        }
        None
    }
}

pub struct EdgesIterMut<'a> {
    offset_graph: &'a mut OffsetGraph,
    current_node_idx: usize,
    current_edge_idx: usize,
}

impl<'a> EdgesIterMut<'a> {
    pub fn new(offset_graph: &'a mut OffsetGraph) -> Self {
        EdgesIterMut {
            offset_graph,
            current_node_idx: 0,
            current_edge_idx: 0,
        }
    }
}

impl<'a> Iterator for EdgesIterMut<'a> {
    type Item = (NodeIDX, &'a mut Edge, &'a mut NonDirectedEdgeMetadata);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_edge_idx >= self.offset_graph.edges.len() {
            return None;
        }

        let mut edge_offset_for_current_node =
            self.offset_graph.edge_offsets[self.current_node_idx + 1];

        // next node might not have any edges, so we increment the node index
        // until we find the one with the offset that is greater than the current edge index
        while self.current_edge_idx >= edge_offset_for_current_node {
            self.current_node_idx += 1;
            if self.current_node_idx >= self.offset_graph.node_count() {
                return None;
            }
            edge_offset_for_current_node =
                self.offset_graph.edge_offsets[self.current_node_idx + 1];
        }

        let parent_idx = NodeIDX::from(self.current_node_idx);

        // Use raw pointers to get around the borrow checker
        let edge = &mut self.offset_graph.edges[self.current_edge_idx] as *mut _;
        let metadata =
            &mut self.offset_graph.non_directed_edges_metadata[self.current_edge_idx] as *mut _;

        self.current_edge_idx += 1;

        unsafe { Some((parent_idx, &mut *edge, &mut *metadata)) }
    }
}

pub struct EdgesIter<'a> {
    offset_graph: &'a OffsetGraph,
    current_node_idx: usize,
    current_edge_idx: usize,
}

impl<'a> EdgesIter<'a> {
    pub fn new(offset_graph: &'a OffsetGraph) -> Self {
        EdgesIter {
            offset_graph,
            current_node_idx: 0,
            current_edge_idx: 0,
        }
    }
}

impl<'a> Iterator for EdgesIter<'a> {
    type Item = (NodeIDX, Edge, &'a NonDirectedEdgeMetadata);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_edge_idx >= self.offset_graph.edges.len() {
            return None;
        }

        let mut edge_offset_for_current_node =
            self.offset_graph.edge_offsets[self.current_node_idx + 1];

        // next node might not have any edges, so we increment the node index
        // until we find the one with the offset that is greater than the current edge index
        while self.current_edge_idx >= edge_offset_for_current_node {
            self.current_node_idx += 1;
            if self.current_node_idx >= self.offset_graph.node_count() {
                return None;
            }
            edge_offset_for_current_node =
                self.offset_graph.edge_offsets[self.current_node_idx + 1];
        }

        let parent_idx = NodeIDX::from(self.current_node_idx);

        // Use raw pointers to get around the borrow checker
        let edge = self.offset_graph.edges[self.current_edge_idx];
        let metadata =
            &self.offset_graph.non_directed_edges_metadata[self.current_edge_idx] as *const _;

        self.current_edge_idx += 1;

        unsafe { Some((parent_idx, edge, &*metadata)) }
    }
}

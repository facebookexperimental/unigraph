// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod edge_flags;
pub(super) mod lengauer_tarjan_dominator_tree;
mod offset_graph_traversal;
mod shortest_path;
use std::collections::BTreeMap;
use std::collections::HashMap;

use anyhow::Result;
use offset_graph_traversal::EdgesIterMut;
use offset_graph_traversal::OffsetGraphDFSConfigured;

use crate::AscendingTier;
use crate::traversal::tiered_traversal::TieredTraversalIter;
use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
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

    pub fn dfs_tiered_configured(
        &self,
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
    ) -> Result<TieredTraversalIter> {
        anyhow::ensure!(tiers.len() <= 4, "Maximum of 4 tiers supported {tiers:?}");

        Ok(TieredTraversalIter::new(self, tiers, entry_points))
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

    pub fn shortest_path_configured(&self, from: &[NodeIDX], to: NodeIDX) -> Option<Vec<NodeIDX>> {
        shortest_path::shortest_path_configured(self, from, to)
    }

    /// Override an edge to exclude it from the graph and returns a struct
    /// containing the original information about the edge so we can restore it later.
    /// this is a VERY dangerous operation and should be used with care.
    /// The idea here is that we can do one off simulations of what the graph would look like
    /// if we included a certain edge and see how it affects the total sizes of the graph.
    /// In JS we used to reconstruct the entire graph for every simulation which would take
    /// seconds to complete. If we accept the mutability, override it, measure, revert the override
    /// we can technically run these in milliseconds and display the results directly in the UI.
    pub fn override_edge_force_include(
        &mut self,
        from_idx: NodeIDX,
        to_idx: NodeIDX,
    ) -> Option<EdgeOverride> {
        let start = self.edge_offsets[from_idx];
        let end = self.edge_offsets[from_idx + 1];

        let edge_idx = (start..end).find(|&idx| self.edges[idx].points_to == to_idx);
        if let Some(idx) = edge_idx {
            let original_edge = self.edges[idx];
            self.edges[idx].flags.remove(EdgeFlags::EXCLUDED);
            Some(EdgeOverride {
                original_edge,
                edge_idx: idx,
            })
        } else {
            None
        }
    }

    pub fn restore_edge_override(&mut self, edge_override: EdgeOverride) {
        self.edges[edge_override.edge_idx] = edge_override.original_edge;
    }
}

#[derive(Debug)]
pub struct EdgeOverride {
    original_edge: Edge,
    edge_idx: usize,
}

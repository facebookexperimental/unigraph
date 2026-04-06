// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod edge_flags;
pub(super) mod lengauer_tarjan_dominator_tree;
mod offset_graph_traversal;
pub(super) mod shortest_path;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::Result;
use offset_graph_traversal::EdgesIterMut;
use offset_graph_traversal::OffsetGraphDFSConfigured;
use rayon::prelude::*;

use crate::AscendingTier;
use crate::traversal::tiered_traversal::TieredTraversalIter;
use crate::types::DynamicBranchName;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::array_graph::offset_graph::offset_graph_traversal::EdgesIter;
use crate::types::array_graph::offset_graph::offset_graph_traversal::OffsetGraphDFSUnconfigured;

/// Wrapper to send raw pointers across rayon threads.
///
/// SAFETY: The caller must ensure no two threads write to overlapping regions.
struct SendPtr<T>(*mut T);
impl<T> Clone for SendPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for SendPtr<T> {}
unsafe impl<T> Send for SendPtr<T> {}
unsafe impl<T> Sync for SendPtr<T> {}

pub struct OffsetGraph {
    pub(crate) edges: Vec<Edge>,
    pub(crate) edge_offsets: Vec<usize>,
    pub(crate) non_directed_edges_metadata: Vec<NonDirectedEdgeMetadata>,
}

#[derive(typegen::TypeGen, Clone, Copy)]
#[typegen(skip(Flow, Hack))]
/// Configured/Unconfigured is borrowed from buck2 terminology.
pub enum TraversalType {
    /// Unconfigured: follow all edges possible, don't care about excluded/included
    Configured = 0,
    /// Configured: follow only the edges that are included, based on
    /// provided TraversalConfig.
    Unconfigured = 1,
}

impl TraversalType {
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(TraversalType::Configured),
            1 => Ok(TraversalType::Unconfigured),
            _ => anyhow::bail!("Invalid TraversalType value: {value}"),
        }
    }
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
        type_key: DynamicTypeKey,
        edge_name: DynamicEdgeName,
        branch: DynamicBranchName,
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

    /// This is a much faster method since it just returns a slice of
    /// the original edges untouched without creating any interators
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

    pub fn iter_edges_mut(&mut self) -> EdgesIterMut<'_> {
        EdgesIterMut::new(self)
    }

    pub fn dfs_tiered_configured(
        &self,
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
    ) -> Result<TieredTraversalIter<'_>> {
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

    /// Build the reverse (transposed) graph using parallel count → prefix-sum →
    /// atomic scatter.
    ///
    /// Three phases:
    ///
    /// 1. **Count in-degrees** — each rayon chunk accumulates a thread-local
    ///    `Vec<usize>` of length `node_count`, then the thread-local vectors are
    ///    merged. Using thread-local counts instead of shared atomics avoids
    ///    contention on high-in-degree hub nodes.
    ///
    /// 2. **Prefix sum** — sequential O(N) scan to compute `reverse_offsets`.
    ///
    /// 3. **Atomic scatter** — parallel loop over source nodes. For each forward
    ///    edge `src → dst`, we write a reverse edge `dst → src` by claiming a
    ///    write slot via `AtomicUsize::fetch_add` on the destination's cursor.
    ///    Each destination's write region is bounded by its in-degree, so no two
    ///    source nodes ever write to the same slot.
    ///
    /// # Why `unsafe`?
    ///
    /// The scatter phase writes into pre-allocated `Vec<Edge>` and
    /// `Vec<NonDirectedEdgeMetadata>` through raw pointers because Rust's borrow
    /// checker cannot prove that the atomic-index-claimed slots are disjoint.
    /// The invariant is maintained by construction: the prefix sum reserves
    /// exactly `in_degree[dst]` slots per destination, and each `fetch_add`
    /// claims exactly one.
    ///
    /// `Edge` is `Copy` (8 bytes, no `Drop`), so we skip zero-initialization
    /// with `set_len()` — every slot is written exactly once during scatter.
    /// `NonDirectedEdgeMetadata` contains `String` fields in some variants, so
    /// it must be initialized to a valid value before the scatter overwrites it
    /// (safe `vec![...; n]`).
    ///
    /// # Peak memory (estimate for 30M nodes, 500M edges)
    ///
    /// | Allocation                        | Size                         | 30M×500M       |
    /// |-----------------------------------|------------------------------|----------------|
    /// | Thread-local in-degree vecs       | `T × N × 8` bytes           | 16 × 240 MB = ~3.8 GB |
    /// | Merged in-degrees                 | `N × 8`                     | 240 MB         |
    /// | Reverse offsets                   | `(N+1) × 8`                 | 240 MB         |
    /// | Reverse edges (`Vec<Edge>`)       | `E × 8`                     | 4.0 GB         |
    /// | Reverse metadata                  | `E × 56` (worst case Dynamic)| 28.0 GB        |
    /// | Atomic cursors                    | `N × 8`                     | 240 MB         |
    /// | **Peak total (excluding input)**  |                              | **~36 GB**     |
    ///
    /// The dominant cost is the reverse metadata vec. If all edges are
    /// `Directed` (the common case), the enum is 1 byte + padding, so actual
    /// memory is much lower (~4 GB for metadata). Thread-local counting adds
    /// `T × N × 8` bytes where T = rayon thread count (typically 8–16).
    /// The thread-local vecs are dropped before the scatter phase begins.
    pub(crate) fn reverse_parallel(&self) -> OffsetGraph {
        let node_count = self.node_count();
        let edge_count = self.edges.len();

        // Phase 1: count in-degree per target node using thread-local arrays.
        let in_degrees = count_in_degrees(&self.edges, node_count, edge_count);

        // Phase 2: prefix sum → reverse_offsets
        let reverse_offsets = prefix_sum(&in_degrees);

        // Phase 3: allocate output arrays.
        // Edge is Copy (no Drop) — skip zero-init since scatter writes every slot.
        // NonDirectedEdgeMetadata has String fields — must be initialized.
        #[allow(clippy::uninit_vec)]
        let mut rev_edges = {
            let mut v = Vec::<Edge>::with_capacity(edge_count);
            // SAFETY: Edge is Copy — no Drop, no invalid bit patterns.
            // All slots are written exactly once in the scatter phase.
            unsafe { v.set_len(edge_count) };
            v
        };
        let mut rev_metadata = vec![NonDirectedEdgeMetadata::Directed; edge_count];

        // Phase 4: scatter reverse edges in parallel via atomic cursors.
        scatter_reverse_edges(
            &self.edges,
            &self.non_directed_edges_metadata,
            &self.edge_offsets,
            &reverse_offsets,
            node_count,
            &mut rev_edges,
            &mut rev_metadata,
        );

        OffsetGraph {
            edges: rev_edges,
            edge_offsets: reverse_offsets,
            non_directed_edges_metadata: rev_metadata,
        }
    }

    /// DFS that will follow only the edges that are not excluded.
    pub fn dfs_configured(&self, roots: &[NodeIDX]) -> OffsetGraphDFSConfigured<'_> {
        OffsetGraphDFSConfigured::new(self, roots)
    }

    pub fn dfs_unconfigured(&self, roots: &[NodeIDX]) -> OffsetGraphDFSUnconfigured<'_> {
        OffsetGraphDFSUnconfigured::new(self, roots)
    }

    pub fn shortest_path(
        &self,
        from: &[NodeIDX],
        to: NodeIDX,
        traversal_type: TraversalType,
    ) -> Option<Vec<NodeIDX>> {
        shortest_path::shortest_path(self, from, to, traversal_type)
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

// ---------------------------------------------------------------------------
// reverse_parallel helpers
// ---------------------------------------------------------------------------

/// Count in-degree per target node using thread-local arrays to avoid
/// atomic contention on high-in-degree hub nodes.
fn count_in_degrees(edges: &[Edge], node_count: usize, edge_count: usize) -> Vec<usize> {
    let chunk_size = (edge_count / rayon::current_num_threads().max(1)).max(1024);

    let thread_local_counts: Vec<Vec<usize>> = edges
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut local = vec![0usize; node_count];
            for edge in chunk {
                local[usize::from(edge.points_to)] += 1;
            }
            local
        })
        .collect();

    let mut merged = vec![0usize; node_count];
    for local in &thread_local_counts {
        for (i, &count) in local.iter().enumerate() {
            merged[i] += count;
        }
    }
    merged
}

/// Sequential prefix sum: `[3, 1, 2]` → `[0, 3, 4, 6]`.
fn prefix_sum(counts: &[usize]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(counts.len() + 1);
    offsets.push(0);
    for &count in counts {
        let last = *offsets.last().unwrap();
        offsets.push(last + count);
    }
    offsets
}

/// Scatter forward edges into reverse positions using per-destination atomic
/// cursors. Each source node is processed in parallel; the atomic `fetch_add`
/// on the destination's cursor guarantees every slot is claimed exactly once.
fn scatter_reverse_edges(
    fwd_edges: &[Edge],
    fwd_metadata: &[NonDirectedEdgeMetadata],
    fwd_offsets: &[usize],
    rev_offsets: &[usize],
    node_count: usize,
    rev_edges: &mut [Edge],
    rev_metadata: &mut [NonDirectedEdgeMetadata],
) {
    let cursors: Vec<AtomicUsize> = rev_offsets[..node_count]
        .iter()
        .map(|&off| AtomicUsize::new(off))
        .collect();

    let edges_ptr = SendPtr(rev_edges.as_mut_ptr());
    let metadata_ptr = SendPtr(rev_metadata.as_mut_ptr());

    (0..node_count).into_par_iter().for_each(|src| {
        let ep = edges_ptr;
        let mp = metadata_ptr;
        let src_idx = NodeIDX::from(src);
        let start = fwd_offsets[src];
        let end = fwd_offsets[src + 1];
        for edge_i in start..end {
            let edge = fwd_edges[edge_i];
            let metadata = &fwd_metadata[edge_i];
            let dest = usize::from(edge.points_to);
            let slot = cursors[dest].fetch_add(1, Ordering::Relaxed);
            // SAFETY: `slot` is in [rev_offsets[dest], rev_offsets[dest+1]).
            // Each fetch_add claims a unique slot within that range (bounded
            // by in_degree[dest] slots reserved by the prefix sum). No two
            // iterations write to the same slot.
            unsafe {
                ep.0.add(slot).write(Edge {
                    points_to: src_idx,
                    flags: edge.flags,
                });
                mp.0.add(slot).write(metadata.clone());
            }
        }
    });
}

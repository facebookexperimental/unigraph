// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod edge_flags;
pub mod edge_overrides;
pub(super) mod lengauer_tarjan_dominator_tree;
mod offset_graph_traversal;
pub(super) mod shortest_path;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::Result;
use rayon::prelude::*;

use crate::AscendingTier;
use crate::EdgeMeta;
use crate::traversal::tiered_traversal::TieredTraversalIter;
use crate::types::EdgeIDX;
use crate::types::EdgeMetaIDX;
use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
pub use crate::types::array_graph::offset_graph::edge_overrides::EdgeOverrides;
pub use crate::types::array_graph::offset_graph::offset_graph_traversal::DFSConfigured;
pub use crate::types::array_graph::offset_graph::offset_graph_traversal::DFSUnconfigured;

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

/// Owned edge graph for derived (reverse/dominator) graphs.
///
/// Stores its own targets, flags, and offsets. Shares metadata with the
/// parent ArrayGraphSerializable via `edge_metadata_map` indices that
/// point into the shared `Vec<EdgeMeta>` table.
pub struct OffsetGraph {
    pub(crate) targets: Vec<NodeIDX>,
    pub(crate) flags: Vec<EdgeFlags>,
    pub(crate) edge_offsets: Vec<usize>,
    /// Sparse index: this graph's edge position → shared metadata table entry.
    /// Empty for dominator graphs (all edges are Directed).
    pub(crate) edge_metadata_map: BTreeMap<EdgeIDX, EdgeMetaIDX>,
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

/// Convenience type for APIs that return materialized edges (e.g. Arrow construction).
/// NOT stored in bulk — use the parallel `targets` + `flags` arrays instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Edge {
    pub points_to: NodeIDX,
    pub flags: EdgeFlags,
}

impl Edge {
    pub fn new(points_to: NodeIDX) -> Self {
        Edge {
            points_to,
            flags: EdgeFlags::empty(),
        }
    }

    pub fn new_with_flags(points_to: NodeIDX, flags: EdgeFlags) -> Self {
        Edge { points_to, flags }
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

// ---------------------------------------------------------------------------
// EdgeGraphView — binds edge data to shared metadata for metadata-aware ops
// ---------------------------------------------------------------------------

/// A borrowed view over edge graph data + the shared metadata table.
///
/// This is the unified way to access edges with metadata across all three
/// graph directions (forward, reverse, dominator). Constructed on-the-fly
/// by `ArrayGraph::edge_view()` — zero cost, just references.
pub struct EdgeGraphView<'a> {
    pub targets: &'a [NodeIDX],
    pub flags: &'a [EdgeFlags],
    pub edge_offsets: &'a [usize],
    pub edge_metadata_map: &'a BTreeMap<EdgeIDX, EdgeMetaIDX>,
    pub metadata_table: &'a [EdgeMeta],
}

impl<'a> EdgeGraphView<'a> {
    pub fn node_count(&self) -> usize {
        self.edge_offsets.len() - 1
    }

    pub fn edges_len(&self) -> usize {
        self.targets.len()
    }

    #[inline(always)]
    pub fn edge_range(&self, node_idx: NodeIDX) -> std::ops::Range<usize> {
        self.edge_offsets[node_idx]..self.edge_offsets[node_idx + 1]
    }

    /// Get edge target + flags for a node as an iterator of (NodeIDX, EdgeFlags).
    #[inline(always)]
    pub fn edges(&self, node_idx: NodeIDX) -> impl Iterator<Item = (NodeIDX, EdgeFlags)> + '_ {
        let range = self.edge_range(node_idx);
        self.targets[range.clone()]
            .iter()
            .zip(&self.flags[range])
            .map(|(&target, &flags)| (target, flags))
    }

    /// Get edges for a node with metadata.
    pub fn edges_with_metadata(
        &self,
        node_idx: NodeIDX,
    ) -> impl Iterator<Item = (Edge, Option<&'a EdgeMeta>)> + '_ {
        let range = self.edge_range(node_idx);
        range.map(move |i| {
            let edge = Edge::new_with_flags(self.targets[i], self.flags[i]);
            let meta = self
                .edge_metadata_map
                .get(&EdgeIDX::from(i))
                .map(|&idx| &self.metadata_table[usize::from(idx)]);
            (edge, meta)
        })
    }

    /// Iterate all edges across all nodes with metadata.
    pub fn iter_edges(&self) -> impl Iterator<Item = (NodeIDX, Edge, Option<&'a EdgeMeta>)> + '_ {
        let node_count = self.node_count();
        (0..node_count).flat_map(move |node| {
            let node_idx = NodeIDX::from(node);
            self.edges_with_metadata(node_idx)
                .map(move |(edge, meta)| (node_idx, edge, meta))
        })
    }

    /// DFS that follows only non-excluded edges.
    pub fn dfs_configured(&self, roots: &[NodeIDX]) -> DFSConfigured<'_> {
        DFSConfigured::new(self.targets, self.flags, self.edge_offsets, roots)
    }

    /// DFS that follows all edges regardless of excluded flags.
    pub fn dfs_unconfigured(&self, roots: &[NodeIDX]) -> DFSUnconfigured<'_> {
        DFSUnconfigured::new(self.targets, self.flags, self.edge_offsets, roots)
    }

    pub fn dfs_tiered_configured(
        &self,
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
    ) -> Result<TieredTraversalIter<'_>> {
        anyhow::ensure!(tiers.len() <= 4, "Maximum of 4 tiers supported {tiers:?}");
        Ok(TieredTraversalIter::new(
            self.targets,
            self.flags,
            self.edge_offsets,
            tiers,
            entry_points,
        ))
    }

    pub fn shortest_path(
        &self,
        from: &[NodeIDX],
        to: NodeIDX,
        traversal_type: TraversalType,
    ) -> Option<Vec<NodeIDX>> {
        shortest_path::shortest_path(
            self.targets,
            self.flags,
            self.edge_offsets,
            from,
            to,
            traversal_type,
        )
    }

    pub fn edges_configured(
        &self,
        node_idx: NodeIDX,
    ) -> impl Iterator<Item = (NodeIDX, EdgeFlags)> + '_ {
        self.edges(node_idx)
            .filter(|(_, flags)| !flags.contains(EdgeFlags::EXCLUDED))
    }
}

// ---------------------------------------------------------------------------
// OffsetGraph — owned edge data for reverse/dominator graphs
// ---------------------------------------------------------------------------

impl OffsetGraph {
    pub fn node_count(&self) -> usize {
        self.edge_offsets.len() - 1
    }

    pub fn edges_len(&self) -> usize {
        self.targets.len()
    }

    pub fn node_idx_iter(&self) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.node_count()).map(NodeIDX::from)
    }

    #[inline(always)]
    pub fn edge_range(&self, node_idx: NodeIDX) -> std::ops::Range<usize> {
        self.edge_offsets[node_idx]..self.edge_offsets[node_idx + 1]
    }

    /// Get edge target + flags for a node.
    pub fn edges(&self, node_idx: NodeIDX) -> impl Iterator<Item = (NodeIDX, EdgeFlags)> + '_ {
        let range = self.edge_range(node_idx);
        self.targets[range.clone()]
            .iter()
            .zip(&self.flags[range])
            .map(|(&target, &flags)| (target, flags))
    }

    /// Get edges for a node filtering excluded.
    pub fn edges_configured(
        &self,
        node_idx: NodeIDX,
    ) -> impl Iterator<Item = (NodeIDX, EdgeFlags)> + '_ {
        self.edges(node_idx)
            .filter(|(_, flags)| !flags.contains(EdgeFlags::EXCLUDED))
    }

    /// Create a view binding this graph to a shared metadata table.
    pub fn view<'a>(&'a self, metadata_table: &'a [EdgeMeta]) -> EdgeGraphView<'a> {
        EdgeGraphView {
            targets: &self.targets,
            flags: &self.flags,
            edge_offsets: &self.edge_offsets,
            edge_metadata_map: &self.edge_metadata_map,
            metadata_table,
        }
    }

    pub fn dfs_configured(&self, roots: &[NodeIDX]) -> DFSConfigured<'_> {
        DFSConfigured::new(&self.targets, &self.flags, &self.edge_offsets, roots)
    }

    pub fn dfs_unconfigured(&self, roots: &[NodeIDX]) -> DFSUnconfigured<'_> {
        DFSUnconfigured::new(&self.targets, &self.flags, &self.edge_offsets, roots)
    }

    pub fn dfs_tiered_configured(
        &self,
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
    ) -> Result<TieredTraversalIter<'_>> {
        anyhow::ensure!(tiers.len() <= 4, "Maximum of 4 tiers supported {tiers:?}");
        Ok(TieredTraversalIter::new(
            &self.targets,
            &self.flags,
            &self.edge_offsets,
            tiers,
            entry_points,
        ))
    }

    pub fn shortest_path(
        &self,
        from: &[NodeIDX],
        to: NodeIDX,
        traversal_type: TraversalType,
    ) -> Option<Vec<NodeIDX>> {
        shortest_path::shortest_path(
            &self.targets,
            &self.flags,
            &self.edge_offsets,
            from,
            to,
            traversal_type,
        )
    }
}

// ---------------------------------------------------------------------------
// reverse_parallel — builds a new OffsetGraph with reversed edges
// ---------------------------------------------------------------------------

/// Build the reverse (transposed) graph using parallel count → prefix-sum →
/// atomic scatter.
///
/// Takes the forward graph's parallel arrays and metadata map. Returns a new
/// OffsetGraph with reversed edges and its own metadata map pointing into the
/// same shared metadata table.
pub(crate) fn reverse_parallel(
    fwd_targets: &[NodeIDX],
    fwd_flags: &[EdgeFlags],
    fwd_offsets: &[usize],
    fwd_metadata_map: &BTreeMap<EdgeIDX, EdgeMetaIDX>,
) -> OffsetGraph {
    let node_count = fwd_offsets.len() - 1;
    let edge_count = fwd_targets.len();

    // Phase 1: count in-degrees
    let in_degrees = count_in_degrees(fwd_targets, node_count, edge_count);

    // Phase 2: prefix sum → reverse_offsets
    let reverse_offsets = prefix_sum(&in_degrees);

    // Phase 3: allocate output arrays.
    #[allow(clippy::uninit_vec)]
    let mut rev_targets = {
        let mut v = Vec::<NodeIDX>::with_capacity(edge_count);
        // SAFETY: NodeIDX is Copy (u32 newtype) — no Drop, no invalid bit patterns.
        // All slots are written exactly once in the scatter phase.
        unsafe { v.set_len(edge_count) };
        v
    };
    #[allow(clippy::uninit_vec)]
    let mut rev_flags = {
        let mut v = Vec::<EdgeFlags>::with_capacity(edge_count);
        // SAFETY: EdgeFlags is Copy (u32 bitflags) — no Drop, no invalid bit patterns.
        unsafe { v.set_len(edge_count) };
        v
    };

    // Phase 4: scatter + build reverse metadata map
    let rev_metadata_map = scatter_reverse_edges(
        fwd_targets,
        fwd_flags,
        fwd_offsets,
        fwd_metadata_map,
        &reverse_offsets,
        node_count,
        &mut rev_targets,
        &mut rev_flags,
    );

    OffsetGraph {
        targets: rev_targets,
        flags: rev_flags,
        edge_offsets: reverse_offsets,
        edge_metadata_map: rev_metadata_map,
    }
}

// ---------------------------------------------------------------------------
// reverse_parallel helpers
// ---------------------------------------------------------------------------

fn count_in_degrees(targets: &[NodeIDX], node_count: usize, edge_count: usize) -> Vec<usize> {
    let chunk_size = (edge_count / rayon::current_num_threads().max(1)).max(1024);

    let thread_local_counts: Vec<Vec<usize>> = targets
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut local = vec![0usize; node_count];
            for &target in chunk {
                local[usize::from(target)] += 1;
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
/// cursors. Also builds the reverse metadata map.
#[allow(clippy::too_many_arguments)]
fn scatter_reverse_edges(
    fwd_targets: &[NodeIDX],
    fwd_flags: &[EdgeFlags],
    fwd_offsets: &[usize],
    fwd_metadata_map: &BTreeMap<EdgeIDX, EdgeMetaIDX>,
    rev_offsets: &[usize],
    node_count: usize,
    rev_targets: &mut [NodeIDX],
    rev_flags: &mut [EdgeFlags],
) -> BTreeMap<EdgeIDX, EdgeMetaIDX> {
    let cursors: Vec<AtomicUsize> = rev_offsets[..node_count]
        .iter()
        .map(|&off| AtomicUsize::new(off))
        .collect();

    let targets_ptr = SendPtr(rev_targets.as_mut_ptr());
    let flags_ptr = SendPtr(rev_flags.as_mut_ptr());

    // Collect metadata mappings from parallel scatter.
    // Each thread collects its own vec; merged after scatter.
    use std::sync::Mutex;
    let metadata_entries: Mutex<Vec<(EdgeIDX, EdgeMetaIDX)>> = Mutex::new(Vec::new());

    (0..node_count).into_par_iter().for_each(|src| {
        let tp = targets_ptr;
        let fp = flags_ptr;
        let src_idx = NodeIDX::from(src);
        let start = fwd_offsets[src];
        let end = fwd_offsets[src + 1];
        let mut local_metadata: Vec<(EdgeIDX, EdgeMetaIDX)> = Vec::new();

        for fwd_edge_i in start..end {
            let dest = usize::from(fwd_targets[fwd_edge_i]);
            let slot = cursors[dest].fetch_add(1, Ordering::Relaxed);
            // SAFETY: `slot` is in [rev_offsets[dest], rev_offsets[dest+1]).
            // Each fetch_add claims a unique slot within that range.
            unsafe {
                tp.0.add(slot).write(src_idx);
                fp.0.add(slot).write(fwd_flags[fwd_edge_i]);
            }
            // If this forward edge has metadata, record the mapping for the reverse slot.
            if let Some(&meta_idx) = fwd_metadata_map.get(&EdgeIDX::from(fwd_edge_i)) {
                local_metadata.push((EdgeIDX::from(slot), meta_idx));
            }
        }

        if !local_metadata.is_empty() {
            metadata_entries.lock().unwrap().extend(local_metadata);
        }
    });

    metadata_entries.into_inner().unwrap().into_iter().collect()
}

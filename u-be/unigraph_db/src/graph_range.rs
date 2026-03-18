// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Ordered range of graph frames for batch operations.
//!
//! Two types:
//!
//! - [`GraphRangeBuilder`]: Accumulates graphs during ingestion. Derives
//!   deltas from consecutive graphs and holds domain data (unpacked graphs
//!   and deltas). Use [`finalize`](GraphRangeBuilder::finalize) or
//!   [`take`](GraphRangeBuilder::take) to produce a [`GraphRange`].
//!
//! - [`GraphRange`]: Finalized, immutable range of domain-level frames
//!   (Full graphs and Deltas). Created by the builder or loaded from storage.
//!
//! ```text
//!   Build:   add(g0)   add(g1)   add(g2)   add_full(g3)  add(g4)
//!            Full      Delta     Delta     Full          Delta
//!               └──base──┘──base──┘           └───base────┘
//!
//!   Replay:  g0        apply(d1) apply(d2) g3            apply(d4)
//!            yield g0  yield g1  yield g2  yield g3      yield g4
//! ```
//!
//! Packing (serialization, compression, blob ID assignment) is NOT done here.
//! It belongs in the storage layer (`cas_store.rs` for store, `load_range.rs`
//! for load).

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::GraphDelta;
use unigraph_core::apply_delta;
use unigraph_core::derive_delta;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineID;

// ---------------------------------------------------------------------------
// GraphRangeFrame
// ---------------------------------------------------------------------------

/// A single frame in a [`GraphRange`] — either a full graph or a delta.
///
/// Both variants hold large heap-allocated data structures, so the enum
/// size difference is not a concern (no point boxing both).
#[allow(clippy::large_enum_variant)]
pub(crate) enum GraphRangeFrame {
    Full(ArrayGraphSerializable),
    Delta(Box<GraphDelta>),
}

// ---------------------------------------------------------------------------
// GraphRangeBuilder
// ---------------------------------------------------------------------------

/// Accumulates graphs into Full/Delta entries during ingestion.
///
/// The first graph is stored as Full. Subsequent graphs are stored as Deltas
/// derived from the previous graph. The previous graph is kept for deriving
/// the next delta.
///
/// Use [`finalize`](Self::finalize) to consume the builder and produce a
/// [`GraphRange`], or [`take`](Self::take) to flush accumulated entries
/// and continue building (used for error recovery).
pub struct GraphRangeBuilder {
    timeline_id: TimelineID,
    entries: Vec<(GraphTimeKey, GraphRangeFrame)>,
    /// The previous graph state, kept for deriving the next delta.
    ///
    /// - `PendingFull`: First graph in a chain. Not yet committed to `entries`
    ///   because we need to borrow it for `derive_delta` when the second graph
    ///   arrives, and then move it into `entries`.
    /// - `ForDelta`: The graph is already referenced in `entries` (as a Full or
    ///   Delta was derived from it). Kept only for deriving the next delta.
    prev: Option<BuilderPrev>,
}

/// Internal state tracking the previous graph during building.
enum BuilderPrev {
    /// First graph in a chain — held back because `derive_delta` needs to
    /// borrow both the previous and new graph simultaneously, and we then
    /// move the previous graph into entries as `Full`.
    PendingFull(GraphTimeKey, ArrayGraphSerializable),
    /// Graph whose entry is already in the list. Kept for delta derivation.
    ForDelta(ArrayGraphSerializable),
}

impl GraphRangeBuilder {
    /// Create an empty builder for the given timeline.
    pub fn new(timeline_id: TimelineID) -> Self {
        Self {
            timeline_id,
            entries: Vec::new(),
            prev: None,
        }
    }

    /// Add a graph to the range.
    ///
    /// The first call holds it as a pending Full. When the second graph
    /// arrives, the pending Full is committed to entries and a Delta is
    /// derived. Subsequent calls derive Deltas from the previous graph.
    pub fn add(&mut self, key: GraphTimeKey, graph: ArrayGraphSerializable) -> Result<()> {
        match self.prev.take() {
            None => {
                // First graph in a chain. Hold as pending until the next arrives.
                self.prev = Some(BuilderPrev::PendingFull(key, graph));
            }
            Some(BuilderPrev::PendingFull(prev_key, prev_graph)) => {
                // Second graph. Derive delta while borrowing both, then commit.
                let delta = derive_delta(&prev_graph, &graph)
                    .context("Failed to derive delta from pending full")?;
                self.entries
                    .push((prev_key, GraphRangeFrame::Full(prev_graph)));
                self.entries
                    .push((key, GraphRangeFrame::Delta(Box::new(delta))));
                self.prev = Some(BuilderPrev::ForDelta(graph));
            }
            Some(BuilderPrev::ForDelta(prev_graph)) => {
                let delta = derive_delta(&prev_graph, &graph).context("Failed to derive delta")?;
                self.entries
                    .push((key, GraphRangeFrame::Delta(Box::new(delta))));
                self.prev = Some(BuilderPrev::ForDelta(graph));
            }
        }
        Ok(())
    }

    /// Force-add a graph as Full, starting a new sub-chain.
    ///
    /// Use this after error recovery or when you want a checkpoint that
    /// doesn't depend on the previous graph for reconstruction.
    pub fn add_full(&mut self, key: GraphTimeKey, graph: ArrayGraphSerializable) -> Result<()> {
        // Flush any pending full first.
        self.flush_pending();
        self.prev = Some(BuilderPrev::PendingFull(key, graph));
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.prev.is_none()
    }

    /// Flush accumulated entries into a [`GraphRange`] and reset the builder.
    ///
    /// The returned range contains all entries accumulated so far. The builder
    /// is reset with a new timeline ID (which may be the same) and can
    /// continue accepting graphs. The next `add()` will produce a Full frame
    /// since `prev` is cleared.
    ///
    /// Used for error recovery: flush the good range, store the error, then
    /// continue building a fresh range.
    pub fn take(&mut self, timeline_id: TimelineID) -> GraphRange {
        self.flush_pending();
        let entries = std::mem::take(&mut self.entries);
        let taken_timeline_id = std::mem::replace(&mut self.timeline_id, timeline_id);
        GraphRange {
            timeline_id: taken_timeline_id,
            entries,
        }
    }

    /// Consume the builder and return the finalized [`GraphRange`].
    ///
    /// Drops `prev` (no longer needed for delta derivation).
    pub fn finalize(mut self) -> GraphRange {
        self.flush_pending();
        GraphRange {
            timeline_id: self.timeline_id,
            entries: self.entries,
        }
    }

    /// If there's a pending Full that hasn't been committed to entries, commit it.
    fn flush_pending(&mut self) {
        if let Some(BuilderPrev::PendingFull(key, graph)) = self.prev.take() {
            self.entries.push((key, GraphRangeFrame::Full(graph)));
        }
        // ForDelta is just kept for deriving the next delta — drop it.
        self.prev = None;
    }
}

// ---------------------------------------------------------------------------
// GraphRange
// ---------------------------------------------------------------------------

/// A finalized range of graph frames holding domain data.
///
/// Created by [`GraphRangeBuilder::finalize`] / [`GraphRangeBuilder::take`],
/// or loaded from storage via `load_range`.
///
/// Use [`replay`](Self::replay) to iterate over reconstructed graphs.
pub struct GraphRange {
    timeline_id: TimelineID,
    entries: Vec<(GraphTimeKey, GraphRangeFrame)>,
}

impl GraphRange {
    /// Construct a range from pre-built entries (used by `load_range`).
    pub(crate) fn from_entries(
        timeline_id: TimelineID,
        entries: Vec<(GraphTimeKey, GraphRangeFrame)>,
    ) -> Self {
        Self {
            timeline_id,
            entries,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn timeline_id(&self) -> &TimelineID {
        &self.timeline_id
    }

    /// Consume the range and return the entries (for storage layer).
    pub(crate) fn into_entries(self) -> (TimelineID, Vec<(GraphTimeKey, GraphRangeFrame)>) {
        (self.timeline_id, self.entries)
    }

    // -----------------------------------------------------------------------
    // Replay
    // -----------------------------------------------------------------------

    /// Replay through the range, reconstructing each graph and calling `f`.
    ///
    /// Consumes the range. Only one full graph is in memory at a time —
    /// the previous graph is consumed by delta application.
    ///
    /// ```text
    ///   Entry:   Full    Delta   Delta   Full    Delta
    ///   Memory:  [g0]    [g1]    [g2]    [g3]    [g4]
    /// ```
    pub fn replay<F>(self, mut f: F) -> Result<()>
    where
        F: FnMut(&GraphTimeKey, &ArrayGraphSerializable) -> Result<()>,
    {
        let mut current: Option<ArrayGraphSerializable> = None;

        for (key, frame) in self.entries {
            let graph = match frame {
                GraphRangeFrame::Full(graph) => graph,
                GraphRangeFrame::Delta(delta) => {
                    let base = current.take().with_context(|| {
                        format!(
                            "delta frame graph_id={} has no preceding full",
                            key.graph_id.0,
                        )
                    })?;
                    apply_delta(base, &delta).with_context(|| {
                        format!("failed to apply delta at graph_id={}", key.graph_id.0)
                    })?
                }
            };
            f(&key, &graph)?;
            current = Some(graph);
        }

        Ok(())
    }
}

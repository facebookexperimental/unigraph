// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Ingestion and maintenance for the plain-row graph metric history.
//!
//! # What this has to cope with
//!
//! The source pipeline registers frames in `graph_id` order and builds them out
//! of order, so at any moment the timeline is pocked with holes: unfilled
//! `Empty` placeholders and failed `Error` builds sitting between real frames.
//! Some fill minutes later, some fill days later, and most never fill at all.
//!
//! # Why that is no longer hard
//!
//! Because a threshold verdict is taken against the **immediately preceding
//! built frame** and nothing else (see [`crate::graph_history::threshold`]), and
//! no frame can ever appear *between* two adjacent frames. Every verdict is
//! therefore final the moment it is taken, and the entire apparatus the old
//! design needed for this — deferred verdicts, propagating provisionality, a
//! settled frontier, a settle window — does not exist here.
//!
//! What a hole costs instead is bounded and local: the built frames on either
//! side keep a row for every node ([`crate::graph_history::gaps`]), and filling
//! one touches exactly three frames.
//!
//! # The shape of a run
//!
//! ```text
//! context::load       every frame + every checkpoint, and the gap flags the
//!                     sequence implies                     (metadata only)
//!   -> sync           write the flags that moved; hand any frame that has
//!                     stopped being a gap's far edge back to the work list
//!   -> work list      everything not yet Ingested, unbounded in time
//!   -> ingest         judge each frame against the frame before it; commit
//!                     rows, anchors and checkpoint in one transaction
//!   -> refresh_latest move the LATEST pin to the newest built frame
//! ```
//!
//! The work list being unbounded is the whole recovery story. The design this
//! replaced only looked at a lookback window, so an ingest outage longer than
//! the window left frames nothing would ever revisit — and because compaction
//! could not pass them, they froze the timeline behind them permanently. Here a
//! frame stays on the list until it is `Ingested`, however long that takes.
//!
//! # What is deliberately cheap
//!
//! Reading every frame's metadata and every checkpoint on each run sounds
//! expensive and is not: both are narrow rows, `www-budget` has ~24k of them
//! after six days, and the previous design already paid the same cost on every
//! `compact` to find its frontier. In exchange, gap structure is recomputed
//! from the sequence every time, so no bookkeeping error survives a run.

mod compact;
mod context;
mod delete;
mod ingest;
mod read;
mod replay;

use std::collections::BTreeMap;

use anyhow::Result;
use ll::task;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::context::UnigraphDbContext;
use crate::graph_history::Reasons;
use crate::graph_history::Values;

/// One frame's per-node metrics, as `extract_node_metrics` returns them.
type NodeMetrics = BTreeMap<String, BTreeMap<String, f64>>;

/// Per-node packed metric values at one frame.
type NodeValues = BTreeMap<String, Values>;

// ── Options ──────────────────────────────────────────────────

/// What `ingest` should look at.
#[derive(Debug, Clone)]
pub struct HistoryIngestOptions {
    /// Optional cap on how far back to reach for outstanding frames.
    ///
    /// Advisory, and `None` by default: ingest always sweeps every frame that
    /// is not yet `Ingested`, however old. Set it only to bound the work one
    /// run will take against a large backlog — the frames it skips stay on the
    /// list for the next run.
    pub lookback_hours: Option<usize>,
    /// Minimum absolute change in any metric, against the immediately
    /// preceding built frame, for a row to be recorded.
    pub threshold: f64,
    /// Optional graph-ID restriction, for repairing a specific range.
    pub graph_id_bounds: GraphIDBounds,
}

/// What `compact` should reclaim and re-threshold.
#[derive(Debug, Clone)]
pub struct HistoryCompactOptions {
    pub threshold: f64,
    pub range: HistoryRange,
}

// ── Reports ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct HistoryIngestReport {
    /// Frames judged and checkpointed this run.
    pub ingested: usize,
    /// Frames that carry no metric values — `Empty` placeholders and failed
    /// builds. They stay on the work list, at no cost beyond this count.
    pub no_data: usize,
    /// Frames already `Ingested`, past the attempt cap, or outside the run's
    /// bounds.
    pub skipped: usize,
    /// Frames history could not read. Retried until
    /// [`MAX_ATTEMPTS`](crate::graph_history::MAX_ATTEMPTS).
    pub errors: usize,
    /// Rows written at the frames being judged.
    pub entries: usize,
    /// Rows written or flagged at the *previous* frame so that a crossing's
    /// step reads as its own diff's contribution.
    pub anchors: usize,
    /// Rows kept unconditionally because their frame bounds a gap. Temporary —
    /// released as soon as the hole closes. A number that climbs run over run
    /// means the source pipeline's gaps are getting worse.
    pub barrier_rows: usize,
    /// Frames whose gap flags moved. Routine at the head of the timeline, where
    /// a newly registered placeholder restates its neighbour's flags.
    pub flags_updated: usize,
    /// Frames handed back to the work list because a gap they bounded closed.
    pub rejudged: usize,
    /// Chunks whose one-pass replay failed and were reconstructed frame by
    /// frame instead. Correct either way, but the fallback is O(L²) delta
    /// applications and L round trips per chunk, so anything above zero on an
    /// `AdjacentDeltas` timeline means the run is paying for a chain the fast
    /// path could not load.
    pub replay_fallbacks: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryCompactReport {
    /// Stretches between barriers that were swept.
    pub segments: usize,
    /// Rows the segment sweep reclaimed — the ones a closing gap released.
    pub collapsed: usize,
    /// Nodes whose series was re-thresholded.
    pub nodes: usize,
    /// Rows the re-threshold pass dropped.
    pub dropped: usize,
    /// Rows whose reasons the re-threshold pass rewrote.
    pub updated: usize,
    /// Frames whose gap flags moved.
    pub flags_updated: usize,
    /// Frames handed back to `ingest` because a gap they bounded closed.
    pub rejudged: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryDeleteReport {
    pub entries_deleted: u64,
    pub statuses_deleted: u64,
    pub metrics_deleted: u64,
    pub error_blobs_registered: usize,
}

/// One row of a node's series, as the read path returns it.
#[derive(Debug, Clone)]
pub struct HistorySeriesRow {
    pub graph_id: GraphID,
    pub timestamp: Timestamp,
    pub values: BTreeMap<String, f64>,
    /// Why the row exists. See [`crate::graph_history::Reasons`].
    pub reasons: Reasons,
    /// Is the previous row in this series the immediately preceding built
    /// frame?
    ///
    /// The question a chart actually has to answer, and not the same as any
    /// single reason bit. A step is attributable to one diff exactly when the
    /// two rows it spans are frame-adjacent — true for an anchor followed by a
    /// crossing, and equally true for two consecutive crossings, which is what
    /// a landing diff stack looks like.
    ///
    /// `false` on a series' first row, and on the first row after a gap.
    pub attributable: bool,
}

// ── Handle ───────────────────────────────────────────────────

#[derive(Clone)]
pub struct GraphHistory {
    pub(crate) ctx: UnigraphDbContext,
}

impl GraphHistory {
    /// Judge every frame that is not yet ingested, oldest first.
    #[task]
    pub async fn ingest(
        &self,
        timeline_id: &TimelineID,
        options: &HistoryIngestOptions,
        task: &ll::Task,
    ) -> Result<HistoryIngestReport> {
        let mut report = HistoryIngestReport::default();
        let mut context = self.load_frame_context(timeline_id, &task).await?;

        let sync = self
            .sync_checkpoints(timeline_id, &mut context, &task)
            .await?;
        report.flags_updated = sync.flags_updated;
        report.rejudged = sync.rejudged;

        let previously_latest = context.latest_ingested();
        let work = context.work_list(options, &mut report)?;
        self.ingest_frames(timeline_id, &work, &context, options, &mut report, &task)
            .await?;
        self.refresh_latest(timeline_id, &context, previously_latest, &task)
            .await?;
        Ok(report)
    }

    /// Reclaim rows nothing needs any more, and re-apply `threshold`.
    ///
    /// Two independent jobs, kept apart because they cost wildly different
    /// amounts:
    ///
    /// - **The segment sweep** deletes zero-reason rows between barriers. One
    ///   statement per segment covering every node at once, so it stays cheap
    ///   however wide the graph is. This is what reclaims the boundary rows a
    ///   closing gap released, and it is the part worth running on a schedule.
    /// - **The re-threshold pass** re-derives each node's crossings and anchors
    ///   from the stored values. Only a threshold change needs it, and it costs
    ///   one series read per node, so bound it with `--min-id` / `--max-id` on
    ///   a wide timeline.
    ///
    /// Compaction can only ever raise the threshold — lowering it would need
    /// values that were never written. Re-ingest for that.
    #[task]
    pub async fn compact(
        &self,
        timeline_id: &TimelineID,
        options: &HistoryCompactOptions,
        task: &ll::Task,
    ) -> Result<HistoryCompactReport> {
        let mut report = HistoryCompactReport::default();
        let mut context = self.load_frame_context(timeline_id, &task).await?;

        let sync = self
            .sync_checkpoints(timeline_id, &mut context, &task)
            .await?;
        report.flags_updated = sync.flags_updated;
        report.rejudged = sync.rejudged;

        self.sweep_segments(timeline_id, &context, options, &mut report, &task)
            .await?;
        self.rethreshold_nodes(timeline_id, &context, options, &mut report, &task)
            .await?;
        Ok(report)
    }

    /// Delete a timeline's recorded history. Frames themselves are untouched.
    #[task]
    pub async fn delete(
        &self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<HistoryDeleteReport> {
        if bounds.0.is_none() && bounds.1.is_none() {
            self.delete_all_history(timeline_id, &task).await
        } else {
            self.delete_bounded(timeline_id, bounds, false, &task).await
        }
    }

    #[task(tags(l3))]
    pub async fn series(
        &self,
        timeline_id: &TimelineID,
        node_name: &str,
        bounds: &TimestampBounds,
        task: &ll::Task,
    ) -> Result<Vec<HistorySeriesRow>> {
        let node_names = [node_name.to_owned()];
        Ok(self
            .series_many(timeline_id, &node_names, bounds, &task)
            .await?
            .remove(node_name)
            .unwrap_or_default())
    }
}

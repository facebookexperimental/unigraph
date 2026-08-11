// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Decoupled graph metric history ingestion and maintenance.
//!
//! # Ingesting against a timeline that fills out of order
//!
//! Frames are registered in `graph_id` order but built out of order (see
//! `build_and_store_www_budget`'s module docs), so at any moment the timeline
//! has holes: unfilled frames sitting between built ones.
//!
//! That breaks the naive threshold filter. It compares a node's sample at `G`
//! against its last *kept* sample at some `P < G` and records `Omitted`, which
//! is never revisited. If a frame in `(P, G)` fills later and is kept, `G` was
//! judged against the wrong neighbour — and the sample is gone for good.
//!
//! So a frame's verdicts are only trusted when the frame before it is *final*,
//! which takes two things (see [`is_frame_final`]):
//!
//! 1. **Settled** — it cannot change any more, so no frame can appear behind
//!    us. See [`crate::graph_history::settle`].
//! 2. **Not itself flagged** — a settled predecessor rules out new *frames*,
//!    but says nothing about whether the row we would measure against
//!    survives. Rows at a flagged frame are provisional, so anything judged
//!    against one is provisional too.
//!
//! Point 2 makes provisionality propagate: once a hole opens, every frame after
//! it stays flagged until `compact` works forward and retires them one by one.
//! A flagged frame keeps every node's row — over-keeping is recoverable,
//! omitting is not — and `compact` reclaims the excess once the gap closes.
//!
//! `compact` needs the same guarantee for the opposite reason (dropping a row
//! is as irreversible as never writing it) but gets it more cheaply: it clamps
//! its range to the settled frontier, so every frame it considers is final by
//! construction.
//!
//! # Anchors: what the threshold throws away
//!
//! A kept sample's absolute values say nothing about which graph moved them.
//! The row before it in the series can be hundreds of frames back, so the
//! obvious reading — "this diff added 99" — actually credits one graph with
//! everything the threshold folded away since the last kept row.
//!
//! ```text
//! ingested   1: 1   2: 2   ...   998: 94   999: 95   1000: 100
//! kept       1: 1                                    1000: 100   "+99"?
//! anchored   1: 1                          999: 95   1000: 100   "+5"
//! ```
//!
//! So keeping a sample also writes the row for the built frame immediately
//! before it, flagged `anchor`. That row is not a threshold crossing and must
//! never be treated as one: it is excluded from baseline lookups (as deferred
//! rows are, and for the same reason — it sits within a threshold of the sample
//! after it, so measuring against it would hide the earlier drift and omit a
//! sample that deserved a row), and compaction neither judges it nor drops it
//! while the sample it explains survives.
//!
//! One consequence worth knowing: a frame checkpointed `Omitted` can still end
//! up with rows, written later by the frame after it. The checkpoint records
//! what that frame's *own* verdict was, not whether the table holds a row for
//! it.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::Context;
use anyhow::Result;
use ll::task;
use unigraph_metric_history::extract_node_metrics;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::HistoryEntryRow;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::HistoryStatusRow;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::context::UnigraphDbContext;
use crate::graph_history::CompactInput;
use crate::graph_history::CompactRow;
use crate::graph_history::ErrorPayload;
use crate::graph_history::HistoryStatus;
use crate::graph_history::MAX_ATTEMPTS;
use crate::graph_history::compact_series;
use crate::graph_history::decode_values;
use crate::graph_history::encode_values;
use crate::graph_history::is_frame_settled;
use crate::graph_history::keep_row;

const DELETE_CHUNK_SIZE: i64 = 10_000;

/// Max status rows written per transaction when stamping Empty frames.
///
/// A timeline can hold tens of thousands of unfilled placeholders, and every
/// status write takes the timeline's exclusive lock. Chunking keeps a single
/// ingest run from blocking the graph pipeline behind one huge transaction.
const STATUS_CHUNK_SIZE: usize = 2_000;

/// How many frames one `load_range` + replay pass covers.
///
/// Replaying a range reconstructs each graph from the previous one, so the
/// whole chunk's unpacked frames are resident while it runs. Bounding the
/// chunk bounds that memory; the only cost of a smaller chunk is re-walking
/// from the preceding Full once per chunk.
const REPLAY_CHUNK_FRAMES: usize = 200;

/// One frame's per-node metrics, as `extract_node_metrics` returns them.
type NodeMetrics = BTreeMap<String, BTreeMap<String, f64>>;

/// Per-node packed metric values at one frame.
type NodeValues = BTreeMap<String, BTreeMap<u32, f64>>;

/// Mutable state carried across the frames of a single ingest run.
struct RunState {
    /// Per node, the last *surviving* sample the threshold measures against.
    /// Deferred rows never enter this — see [`GraphHistory::process_frame`].
    last_kept: HashMap<String, BTreeMap<u32, f64>>,
    /// The previous built frame, as far as this run knows it.
    ///
    /// `None` means its metrics are not in hand — the run has not reached a
    /// frame it processed yet, or it skipped over one — and anchors cannot be
    /// minted until [`GraphHistory::seed_prev_frame`] recovers it.
    prev_frame: Option<PrevFrame>,
    /// The timeline's `metric name -> id` dictionary.
    ///
    /// Held across the run because interning takes the timeline's exclusive
    /// lock, and after the first frame it has nothing to do — the dictionary
    /// is a handful of names that never change. Re-interning per frame cost a
    /// write transaction per frame and contended with graph ingestion.
    metric_ids: BTreeMap<String, u32>,
}

/// Everything the next frame needs to mint anchors against its predecessor.
///
/// Costs about what `last_kept` costs: one packed value map per node. That is
/// the price of being able to write the predecessor's row after the fact —
/// nothing else in the system remembers what a below-threshold frame held.
struct PrevFrame {
    graph_id: GraphID,
    timestamp: Timestamp,
    values: NodeValues,
    /// Nodes that already have a row at this frame, and so need no anchor.
    has_row: HashSet<String>,
}

/// The rows one processed frame contributes, and what they are.
struct FrameEntries {
    rows: Vec<HistoryEntryRow>,
    /// Rows recorded *at this frame* — kept plus deferred, excluding anchors,
    /// which belong to the frame before.
    samples: usize,
    /// How many of `samples` were below the threshold and kept anyway.
    deferred_rows: usize,
    /// Rows minted at the previous frame to explain a sample kept at this one.
    anchors: usize,
}

/// What `ingest` should look at, and how strictly it may filter.
#[derive(Debug, Clone)]
pub struct HistoryIngestOptions {
    /// How far back from now to scan for frames to ingest.
    pub lookback_hours: usize,
    /// Age at which an unfilled frame is presumed abandoned. Frames behind a
    /// younger hole are ingested but not threshold-filtered.
    /// See [`crate::graph_history::DEFAULT_SETTLE_HOURS`].
    pub settle_hours: usize,
    /// Minimum absolute change in any metric, versus the node's last kept
    /// sample, for a new sample to be recorded.
    pub threshold: f64,
    /// Optional graph-ID restriction, for repairing a specific range.
    pub graph_id_bounds: GraphIDBounds,
}

/// What `compact` should re-threshold.
#[derive(Debug, Clone)]
pub struct HistoryCompactOptions {
    pub threshold: f64,
    pub settle_hours: usize,
    pub range: HistoryRange,
    /// Ignore `range`'s graph-ID bounds and reconsider exactly the frames
    /// still flagged `omission_deferred` — the ones ingest could not judge
    /// against a trustworthy baseline. This is the incremental mode a scheduled
    /// job wants: its cost scales with flagged frames, not with node count.
    pub deferred_only: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryIngestReport {
    pub processed: usize,
    pub omitted: usize,
    pub empty: usize,
    pub skipped: usize,
    pub errors: usize,
    pub entries: usize,
    /// Frames whose verdicts were provisional — either they sat behind an
    /// unfilled hole, or the frame before them was itself provisional. Their
    /// rows are all retained until `compact` revisits them. A persistently
    /// non-zero count means the source pipeline is lagging further behind than
    /// `settle_hours`, and history is holding far more rows than it needs to.
    pub deferred: usize,
    /// Rows written only because the threshold could not be trusted, summed
    /// over the run. This is the storage `compact` is expected to reclaim, and
    /// the number to watch if write amplification becomes a concern.
    pub deferred_rows: usize,
    /// Rows written at a frame the threshold had already folded away, so that
    /// the sample after it reads as its own graph's contribution. Bounded above
    /// by the number of kept rows, and the other number to watch for write
    /// amplification — unlike `deferred_rows`, compaction will not reclaim it.
    pub anchors: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryCompactReport {
    pub nodes: usize,
    pub dropped: usize,
    /// Redundant rows kept as anchors instead of being deleted, because the
    /// frame right after them holds a surviving sample.
    pub anchored: usize,
    /// Highest graph ID considered. Compaction stops at the settled frontier,
    /// so this trails the timeline head by roughly `settle_hours`.
    pub compacted_through: Option<GraphID>,
}

/// What compacting one node's series actually changed.
#[derive(Debug, Clone, Default)]
struct CompactCounts {
    dropped: usize,
    anchored: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryDeleteReport {
    pub entries_deleted: u64,
    pub statuses_deleted: u64,
    pub metrics_deleted: u64,
    pub error_blobs_registered: usize,
}

#[derive(Debug, Clone)]
pub struct HistorySeriesRow {
    pub graph_id: GraphID,
    pub timestamp: Timestamp,
    pub values: BTreeMap<String, f64>,
    /// The row is not a threshold crossing in its own right — it is the built
    /// frame immediately before the next row in the series, kept so that row's
    /// step can be attributed to its own graph. See
    /// [`unigraph_storage_core::HistoryEntryRow::anchor`].
    pub anchor: bool,
}

struct CommitSummary {
    status: HistoryStatus,
    entries: usize,
    /// How many of `entries` were below the threshold and kept anyway.
    deferred_rows: usize,
    /// Rows written at the *previous* frame to explain a sample kept here.
    anchors: usize,
    /// The frame's verdicts were taken against a baseline that could still
    /// change, because an earlier frame was unfilled. Every row here — kept or
    /// omitted — is provisional until `compact` revisits the frame.
    deferred: bool,
}

#[derive(Clone)]
pub struct GraphHistory {
    pub(crate) ctx: UnigraphDbContext,
}

impl GraphHistory {
    #[task]
    pub async fn ingest(
        &self,
        timeline_id: &TimelineID,
        options: &HistoryIngestOptions,
        task: &ll::Task,
    ) -> Result<HistoryIngestReport> {
        let frames = self.frames_in_lookback(timeline_id, options, &task).await?;
        self.ingest_frames(timeline_id, frames, options, &task)
            .await
    }

    /// Re-apply `threshold` to already-ingested rows.
    ///
    /// Only touches the settled prefix of the timeline: a row dropped here can
    /// never come back, so a frame that might still fill behind it must not
    /// influence the decision. That clamp is what makes this safe to run
    /// incrementally behind `ingest`.
    ///
    /// Two strategies, picked by [`HistoryCompactOptions::deferred_only`]:
    ///
    /// - **Per frame** — walks only the frames ingest flagged, reconsidering
    ///   every row at each. Costs one pass per flagged frame rather than one
    ///   per node, which is what a job running behind `ingest` wants.
    /// - **Per node** — walks every node's whole series in the range. Only
    ///   needed when the threshold itself changed, since that can also
    ///   invalidate rows no frame ever flagged.
    #[task]
    pub async fn compact(
        &self,
        timeline_id: &TimelineID,
        options: &HistoryCompactOptions,
        task: &ll::Task,
    ) -> Result<HistoryCompactReport> {
        let Some(range) = self
            .resolve_compact_range(timeline_id, options, &task)
            .await?
        else {
            return Ok(HistoryCompactReport::default());
        };

        if options.deferred_only {
            self.compact_deferred_frames(timeline_id, options.threshold, &range, &task)
                .await
        } else {
            self.compact_every_node(timeline_id, options.threshold, &range, &task)
                .await
        }
    }

    #[task]
    pub async fn delete(
        &self,
        timeline_id: &TimelineID,
        bounds: &(Option<GraphID>, Option<GraphID>),
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

    /// Read several nodes' series in one pass, keyed by node name.
    ///
    /// The metric-name dictionary and the connection are read once for the
    /// whole batch rather than once per node. Duplicate names in `node_names`
    /// collapse to a single entry.
    #[task(tags(l3))]
    pub async fn series_many(
        &self,
        timeline_id: &TimelineID,
        node_names: &[String],
        bounds: &TimestampBounds,
        task: &ll::Task,
    ) -> Result<BTreeMap<String, Vec<HistorySeriesRow>>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let metric_names = conn.get_history_metric_names(timeline_id, &task).await?;
        let range = HistoryRange {
            timestamps: bounds.clone(),
            graph_ids: (None, None),
        };

        let mut out = BTreeMap::new();
        for node_name in node_names {
            let rows = conn
                .get_history_series(timeline_id, node_name, &range, &task)
                .await?;
            let series = rows
                .into_iter()
                .map(|row| {
                    Ok(HistorySeriesRow {
                        graph_id: row.graph_id,
                        timestamp: row.timestamp,
                        values: decode_named_values(&metric_names, &row.values)?,
                        anchor: row.anchor,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            out.insert(node_name.clone(), series);
        }
        Ok(out)
    }
}

impl GraphHistory {
    async fn frames_in_lookback(
        &self,
        timeline_id: &TimelineID,
        options: &HistoryIngestOptions,
        task: &ll::Task,
    ) -> Result<Vec<FrameRow>> {
        let now = Timestamp::now();
        let start = now
            .subtract_hours(options.lookback_hours)
            .context("lookback_hours is too large")?;

        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.get_timeline_config(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        drop(conn);

        self.select_history_frames(
            timeline_id,
            TimestampBounds {
                start: Some(start),
                end: Some(now),
            },
            options.graph_id_bounds,
            task,
        )
        .await
    }

    async fn select_history_frames(
        &self,
        timeline_id: &TimelineID,
        bounds: TimestampBounds,
        graph_id_bounds: GraphIDBounds,
        task: &ll::Task,
    ) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                limit: None,
                frame_types: Some(vec![
                    FrameType::Empty,
                    FrameType::Full,
                    FrameType::Delta,
                    FrameType::Error,
                ]),
                order: Some(Order::Asc),
                timestamp_bounds: Some(bounds),
                graph_id_bounds: Some(graph_id_bounds),
                graph_ids: None,
                with_data: Some(false),
                before: None,
                expires_before: None,
            },
            task,
        )
        .await
    }

    /// The frame immediately before `frame` in the timeline, if any.
    ///
    /// Needed for the first frame of a scan window: its predecessor sits
    /// outside the window, but whether that predecessor is settled decides
    /// whether the window's first frame may omit anything.
    async fn preceding_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        task: &ll::Task,
    ) -> Result<Option<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: timeline_id.clone(),
                    before: Some((frame.frame.timestamp, frame.frame.graph_id)),
                    limit: Some(1),
                    with_data: Some(false),
                    ..Default::default()
                },
                task,
            )
            .await?;
        Ok(rows.pop())
    }

    /// Highest graph ID whose entire prefix has stopped changing.
    ///
    /// `None` means even the oldest frame is still in flux, so nothing can be
    /// safely compacted yet. Only Full/Delta frames need a status lookup —
    /// Empty settles on age and Error is terminal on sight — which keeps this
    /// to a handful of queries even on a timeline that is mostly placeholders.
    async fn settled_frontier(
        &self,
        timeline_id: &TimelineID,
        settle_cutoff: Timestamp,
        task: &ll::Task,
    ) -> Result<Option<GraphID>> {
        let frames = self
            .select_history_frames(timeline_id, TimestampBounds::default(), (None, None), task)
            .await?;

        let built_ids = frames
            .iter()
            .filter(|frame| matches!(frame.frame_type, FrameType::Full | FrameType::Delta))
            .map(|frame| frame.frame.graph_id)
            .collect::<Vec<_>>();
        let mut conn = self.ctx.storage.graph.conn().await?;
        let status_by_id = conn
            .get_history_status(timeline_id, &built_ids, task)
            .await?
            .into_iter()
            .map(|row| (row.graph_id, row))
            .collect::<BTreeMap<_, _>>();
        drop(conn);

        let mut frontier = None;
        for frame in &frames {
            let settled = is_frame_settled(
                &frame.frame_type,
                frame.frame.timestamp,
                status_by_id.get(&frame.frame.graph_id),
                settle_cutoff,
            );
            if !settled {
                break;
            }
            frontier = Some(frame.frame.graph_id);
        }
        Ok(frontier)
    }

    /// Intersect the caller's request with the deferred work list and the
    /// settled frontier. `None` means there is nothing safe to compact.
    async fn resolve_compact_range(
        &self,
        timeline_id: &TimelineID,
        options: &HistoryCompactOptions,
        task: &ll::Task,
    ) -> Result<Option<HistoryRange>> {
        let settle_cutoff = settle_cutoff(options.settle_hours)?;
        let Some(frontier) = self
            .settled_frontier(timeline_id, settle_cutoff, task)
            .await?
        else {
            task.data("history_compact_skipped", "no settled frames");
            return Ok(None);
        };

        let mut range = options.range.clone();
        if options.deferred_only {
            let mut conn = self.ctx.storage.graph.conn().await?;
            let Some(deferred) = conn.get_history_deferred_bounds(timeline_id, task).await? else {
                task.data("history_compact_skipped", "no deferred frames");
                return Ok(None);
            };
            range.graph_ids = deferred;
        }

        range.graph_ids.1 = Some(match range.graph_ids.1 {
            Some(upper) => upper.min(frontier),
            None => frontier,
        });
        if let (Some(lower), Some(upper)) = range.graph_ids
            && lower > upper
        {
            task.data("history_compact_skipped", "range is behind the frontier");
            return Ok(None);
        }
        Ok(Some(range))
    }

    /// Retire the deferral bookkeeping for a range that has just been compacted.
    ///
    /// Every row still present in the range survived the threshold, so it is a
    /// baseline row now and must become visible to future baseline lookups.
    /// One statement for the whole range rather than one per node — the flag is
    /// cleared for survivors only because the drops already happened.
    async fn clear_deferred(
        &self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<()> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.clear_history_entries_deferred(timeline_id, bounds, task)
            .await?;
        conn.clear_history_omission_deferred(timeline_id, bounds, task)
            .await?;
        conn.commit_transaction(task).await
    }

    /// Walk the window in `graph_id` order, ingesting what needs it.
    ///
    /// The walk is stateful in one crucial way: `prev_settled` tracks whether
    /// the frame just visited can still change. That is what licenses the next
    /// frame to omit anything, and it has to be recomputed as we go — a frame
    /// that was pending when the run started is settled by the time we reach
    /// its successor, so a snapshot taken up front would needlessly defer the
    /// whole window.
    async fn ingest_frames(
        &self,
        timeline_id: &TimelineID,
        frames: Vec<FrameRow>,
        options: &HistoryIngestOptions,
        task: &ll::Task,
    ) -> Result<HistoryIngestReport> {
        let mut report = HistoryIngestReport::default();
        let status_by_id = self.load_statuses(timeline_id, &frames, task).await?;
        self.mark_empty_frames(timeline_id, &frames, &status_by_id, &mut report, task)
            .await?;

        let settle_cutoff = settle_cutoff(options.settle_hours)?;
        let mut prev_final = self
            .window_starts_final(timeline_id, &frames, settle_cutoff, task)
            .await?;
        let mut run = RunState {
            last_kept: HashMap::new(),
            prev_frame: None,
            metric_ids: BTreeMap::new(),
        };

        let total = i64::try_from(frames.len())?;
        task.data("frames_in_window", total);
        task.progress(0, total);

        let replayable = self.timeline_supports_replay(timeline_id, task).await?;
        let mut done = 0i64;

        for chunk in frames.chunks(REPLAY_CHUNK_FRAMES) {
            let mut graphs = self
                .extract_chunk(timeline_id, chunk, &status_by_id, replayable, task)
                .await;

            for frame in chunk {
                let existing_status = status_by_id.get(&frame.frame.graph_id);
                let stored_status = match frame_action(frame, existing_status) {
                    FrameAction::Skip => {
                        report.skipped += 1;
                        // Skipping a built frame loses its metrics, so the next
                        // frame has no predecessor to anchor against until it
                        // re-seeds. Empty and Error frames carry none to begin
                        // with — the last built frame is still the predecessor.
                        if matches!(frame.frame_type, FrameType::Full | FrameType::Delta) {
                            run.prev_frame = None;
                        }
                        existing_status.cloned()
                    }
                    FrameAction::Process => {
                        self.process_and_record(
                            timeline_id,
                            frame,
                            options.threshold,
                            prev_final,
                            existing_status,
                            graphs.remove(&frame.frame.graph_id),
                            &mut run,
                            &mut report,
                            task,
                        )
                        .await
                    }
                };
                prev_final = is_frame_final(frame, stored_status.as_ref(), settle_cutoff);
                done += 1;
                task.progress(done, total);
            }
        }

        Ok(report)
    }

    /// Whether this timeline's frames can be replayed as a range.
    ///
    /// Only `AdjacentDeltas` chains a delta to the frame before it, which is
    /// what lets one pass reconstruct every graph. Other schemas fall back to
    /// fetching each frame independently.
    async fn timeline_supports_replay(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<bool> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let config = conn.get_timeline_config(timeline_id, task).await?;
        Ok(matches!(
            config.map(|config| config.schema),
            Some(TimelineSchema::AdjacentDeltas(_))
        ))
    }

    /// Reconstruct a chunk's graphs in one pass and return their metrics.
    ///
    /// Fetching frames one at a time re-walks from the nearest Full for every
    /// frame, so a chain of length L costs O(L²) delta applications and L
    /// round trips. `load_range` pulls the chain in a single query — snapping
    /// back to the Full itself — and `replay` folds each graph out of the
    /// previous one, making it O(L).
    ///
    /// Best-effort: any failure falls back to the per-frame fetch path rather
    /// than failing the run, so an odd chain degrades in speed, not
    /// correctness.
    async fn extract_chunk(
        &self,
        timeline_id: &TimelineID,
        chunk: &[FrameRow],
        status_by_id: &BTreeMap<GraphID, HistoryStatusRow>,
        replayable: bool,
        task: &ll::Task,
    ) -> BTreeMap<GraphID, NodeMetrics> {
        if !replayable {
            return BTreeMap::new();
        }
        let wanted = chunk
            .iter()
            .filter(|frame| {
                matches!(
                    frame_action(frame, status_by_id.get(&frame.frame.graph_id)),
                    FrameAction::Process
                )
            })
            .map(|frame| frame.frame.graph_id)
            .collect::<BTreeSet<_>>();

        let (Some(from), Some(to)) = (wanted.first(), wanted.last()) else {
            return BTreeMap::new();
        };
        match self
            .replay_metrics(timeline_id, *from, *to, &wanted, task)
            .await
        {
            Ok(graphs) => graphs,
            Err(error) => {
                task.data("history_replay_fallback", format!("{error:#}"));
                BTreeMap::new()
            }
        }
    }

    async fn replay_metrics(
        &self,
        timeline_id: &TimelineID,
        from: GraphID,
        to: GraphID,
        wanted: &BTreeSet<GraphID>,
        task: &ll::Task,
    ) -> Result<BTreeMap<GraphID, NodeMetrics>> {
        let range = crate::schemas::adjacent_deltas::load_range(
            timeline_id,
            &self.ctx,
            Some(from),
            Some(to),
            task,
        )
        .await?;

        let mut graphs = BTreeMap::new();
        range.replay(|key, graph| {
            if wanted.contains(&key.graph_id) {
                graphs.insert(key.graph_id, extract_node_metrics(graph));
            }
            Ok(())
        })?;
        Ok(graphs)
    }

    /// Whether the frame *preceding* the scan window is final — the seed for
    /// the `prev_final` walk. A window starting at the very first frame of the
    /// timeline has nothing behind it, so it is trivially final.
    async fn window_starts_final(
        &self,
        timeline_id: &TimelineID,
        frames: &[FrameRow],
        settle_cutoff: Timestamp,
        task: &ll::Task,
    ) -> Result<bool> {
        let Some(first) = frames.first() else {
            return Ok(true);
        };
        let Some(preceding) = self.preceding_frame(timeline_id, first, task).await? else {
            return Ok(true);
        };

        let mut conn = self.ctx.storage.graph.conn().await?;
        let status = conn
            .get_history_status(timeline_id, &[preceding.frame.graph_id], task)
            .await?
            .pop();
        Ok(is_frame_final(&preceding, status.as_ref(), settle_cutoff))
    }

    /// Ingest one frame, recording either its verdict or its failure. Returns
    /// the checkpoint now stored for it, which decides whether the *next*
    /// frame may omit anything.
    #[expect(
        clippy::too_many_arguments,
        reason = "a params struct here would just re-list the same borrows with a name"
    )]
    async fn process_and_record(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        threshold: f64,
        omissible: bool,
        existing_status: Option<&HistoryStatusRow>,
        extracted: Option<NodeMetrics>,
        run: &mut RunState,
        report: &mut HistoryIngestReport,
        task: &ll::Task,
    ) -> Option<HistoryStatusRow> {
        let attempts = existing_status.map_or(0, |status| status.attempts);
        match self
            .process_frame(
                timeline_id,
                frame,
                threshold,
                omissible,
                existing_status,
                extracted,
                run,
                task,
            )
            .await
        {
            Ok(summary) => {
                let stored = HistoryStatusRow {
                    graph_id: frame.frame.graph_id,
                    status: summary.status.to_string(),
                    attempts,
                    error_blob_key: None,
                    omission_deferred: summary.deferred,
                };
                apply_commit_summary(report, summary);
                Some(stored)
            }
            Err(error) => {
                report.errors += 1;
                if let Err(record_error) = self
                    .record_error(timeline_id, frame, existing_status, error, task)
                    .await
                {
                    task.data(
                        "history_error_status_failure",
                        format!("{}: {record_error:#}", frame.frame.graph_id.0),
                    );
                    return None;
                }
                Some(HistoryStatusRow {
                    graph_id: frame.frame.graph_id,
                    status: HistoryStatus::Error.to_string(),
                    attempts: attempts + 1,
                    error_blob_key: None,
                    omission_deferred: false,
                })
            }
        }
    }

    /// Fetch, extract, threshold and commit one frame.
    ///
    /// `omissible` is the gate: when false, every node's row is written
    /// regardless of the threshold and the checkpoint is flagged so `compact`
    /// re-applies it later. Rows are cheap to delete and impossible to recover,
    /// so this is the direction to err in.
    #[expect(
        clippy::too_many_arguments,
        reason = "a params struct here would just re-list the same borrows with a name"
    )]
    async fn process_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        threshold: f64,
        omissible: bool,
        existing_status: Option<&HistoryStatusRow>,
        extracted: Option<NodeMetrics>,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<CommitSummary> {
        // Taken before anything can fail, so an early return leaves it cleared:
        // a frame we could not read is still the next frame's predecessor, and
        // we no longer know what it held.
        let carried = run.prev_frame.take();

        let extracted = match extracted {
            Some(extracted) => extracted,
            None => {
                self.fetch_and_extract(timeline_id, frame.frame.graph_id, task)
                    .await?
            }
        };
        self.refresh_metric_ids(timeline_id, &extracted, run, task)
            .await?;
        let mut current_rows = metric_snapshots_to_ids(&extracted, &run.metric_ids)?;

        let prev_frame = match carried {
            Some(prev) => Some(prev),
            None => self.seed_prev_frame(timeline_id, frame, run, task).await,
        };

        self.prime_last_kept(
            timeline_id,
            frame.frame.graph_id,
            &current_rows,
            &mut run.last_kept,
            task,
        )
        .await?;

        // Every node the run has seen gets an entry so a node that vanished
        // from the graph records a zeroed sample rather than dangling.
        for node_name in run.last_kept.keys() {
            current_rows.entry(node_name.clone()).or_default();
        }

        let entries = threshold_frame(
            frame,
            &current_rows,
            prev_frame.as_ref(),
            &mut run.last_kept,
            threshold,
            omissible,
        );
        let written = entries
            .rows
            .iter()
            .filter(|row| row.graph_id == frame.frame.graph_id)
            .map(|row| row.node_name.clone())
            .collect();
        run.prev_frame = Some(PrevFrame {
            graph_id: frame.frame.graph_id,
            timestamp: frame.frame.timestamp,
            values: current_rows,
            has_row: written,
        });

        self.commit_processed_frame(
            timeline_id,
            frame,
            entries,
            !omissible,
            existing_status,
            task,
        )
        .await
    }

    /// Reconstruct the previous built frame so anchors can be minted at a run
    /// boundary.
    ///
    /// A scheduled run's window is a prefix of already-ingested frames followed
    /// by the new ones, and skipping that prefix leaves `prev_frame` empty at
    /// exactly the frame that matters. Without this a job ingesting one frame
    /// per pass — which is the normal shape — would never mint an anchor.
    ///
    /// Best-effort: one graph reconstruction, and a failure costs the anchor,
    /// not the ingest.
    async fn seed_prev_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Option<PrevFrame> {
        match self.load_prev_frame(timeline_id, frame, run, task).await {
            Ok(prev) => prev,
            Err(error) => {
                task.data("history_anchor_seed_failed", format!("{error:#}"));
                None
            }
        }
    }

    async fn load_prev_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<Option<PrevFrame>> {
        let Some(preceding) = self
            .preceding_built_frame(timeline_id, frame.frame.graph_id, task)
            .await?
        else {
            return Ok(None);
        };
        task.data("history_anchor_seed", preceding.frame.graph_id.0);

        let extracted = self
            .fetch_and_extract(timeline_id, preceding.frame.graph_id, task)
            .await?;
        self.refresh_metric_ids(timeline_id, &extracted, run, task)
            .await?;

        let mut conn = self.ctx.storage.graph.conn().await?;
        let has_row = conn
            .get_history_entries_at(timeline_id, preceding.frame.graph_id, task)
            .await?
            .into_iter()
            .map(|sample| sample.node_name)
            .collect();

        Ok(Some(PrevFrame {
            graph_id: preceding.frame.graph_id,
            timestamp: preceding.frame.timestamp,
            values: metric_snapshots_to_ids(&extracted, &run.metric_ids)?,
            has_row,
        }))
    }

    /// The nearest `Full`/`Delta` frame before `graph_id`.
    ///
    /// Empty and Error frames are skipped: "the graph immediately before" means
    /// the last one that actually carried metrics.
    async fn preceding_built_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<Option<FrameRow>> {
        self.nearest_built_frame(
            timeline_id,
            (None, Some(GraphID(graph_id.0 - 1))),
            Order::Desc,
            task,
        )
        .await
    }

    /// The nearest `Full`/`Delta` frame after `graph_id`.
    async fn next_built_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<Option<FrameRow>> {
        self.nearest_built_frame(
            timeline_id,
            (Some(GraphID(graph_id.0 + 1)), None),
            Order::Asc,
            task,
        )
        .await
    }

    async fn nearest_built_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id_bounds: GraphIDBounds,
        order: Order,
        task: &ll::Task,
    ) -> Result<Option<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: timeline_id.clone(),
                    limit: Some(1),
                    frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                    order: Some(order),
                    timestamp_bounds: None,
                    graph_id_bounds: Some(graph_id_bounds),
                    graph_ids: None,
                    with_data: Some(false),
                    before: None,
                    expires_before: None,
                },
                task,
            )
            .await?;
        Ok(rows.pop())
    }

    /// Fall back to reconstructing one frame on its own.
    ///
    /// Used when the chunk replay could not supply the graph — a non-
    /// `AdjacentDeltas` timeline, or a range that failed to load.
    async fn fetch_and_extract(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<NodeMetrics> {
        let graph = self
            .ctx
            .storage
            .fetch_graph(
                &GraphKey {
                    timeline_id: timeline_id.clone(),
                    graph_id,
                },
                task,
            )
            .await?;
        Ok(extract_node_metrics(&graph))
    }

    /// Make sure every metric name in `extracted` has an id, interning only if
    /// one is genuinely new.
    ///
    /// Interning takes the timeline's exclusive lock, so doing it per frame
    /// meant a write transaction per frame for a dictionary that stops
    /// changing after the first one. A stale cache is self-correcting: the
    /// only way it hurts is a missing name, and that is exactly what triggers
    /// the re-read.
    async fn refresh_metric_ids(
        &self,
        timeline_id: &TimelineID,
        extracted: &NodeMetrics,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<()> {
        let complete = extracted
            .values()
            .flat_map(BTreeMap::keys)
            .all(|name| run.metric_ids.contains_key(name));
        if complete {
            return Ok(());
        }
        run.metric_ids = self
            .intern_metric_names(timeline_id, extracted, task)
            .await?;
        Ok(())
    }

    /// Assign a stable id to every metric name present in `extracted`, and
    /// return the timeline's full `name -> metric_id` dictionary.
    ///
    /// Runs in its own short transaction. New ids are allocated as
    /// `MAX(metric_id) + 1` *at statement execution time*, so two writers
    /// interning different names for the same timeline concurrently would
    /// otherwise be free to compute the same next id. The
    /// `(timeline_id, metric_id)` primary key means that surfaces as a
    /// constraint error rather than silent corruption, but serializing the
    /// allocation avoids the intermittent failure entirely — and makes the
    /// single-writer requirement explicit instead of leaning on whatever
    /// exclusivity `conn_write()` happens to provide on a given backend.
    ///
    /// Deliberately kept out of the per-frame insert transaction: interning
    /// only ever appends to a tiny dictionary, so committing it early is safe
    /// even if the frame itself later fails.
    async fn intern_metric_names(
        &self,
        timeline_id: &TimelineID,
        extracted: &NodeMetrics,
        task: &ll::Task,
    ) -> Result<BTreeMap<String, u32>> {
        let names = extracted
            .values()
            .flat_map(|metrics| metrics.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        let metric_ids = conn
            .intern_history_metrics(timeline_id, &names, task)
            .await?;
        conn.commit_transaction(task).await?;
        Ok(metric_ids)
    }

    /// Fill in `last_kept` from the DB for nodes this run hasn't seen yet.
    ///
    /// `last_kept` is the run's rolling "what did we last write for this node"
    /// map — the threshold compares against it, not against the previous
    /// frame. It starts empty, so without this a node's first appearance in
    /// the run would look like a brand-new node and always be kept.
    ///
    /// That's wrong whenever the node already has history from an earlier run:
    /// a scheduled `--lookback-hours 1` pass would otherwise write a fresh row
    /// for every node on its very first frame, regardless of threshold. So for
    /// each node in `current_rows` that isn't in the map yet, we look up its
    /// most recent kept entry strictly *before* this frame and seed it.
    ///
    /// Nodes with no earlier entry are left absent, which is what genuinely
    /// makes them new — `keep_row(None, ..)` keeps the first sample.
    ///
    /// Called per frame rather than once up front because the node set can
    /// grow as the run walks forward; the `missing` filter keeps repeat calls
    /// cheap (a no-op once every current node is already primed).
    async fn prime_last_kept(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        current_rows: &BTreeMap<String, BTreeMap<u32, f64>>,
        last_kept: &mut HashMap<String, BTreeMap<u32, f64>>,
        task: &ll::Task,
    ) -> Result<()> {
        let missing = current_rows
            .keys()
            .filter(|node_name| !last_kept.contains_key(*node_name))
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        let mut conn = self.ctx.storage.graph.conn().await?;
        for (node_name, values) in conn
            .get_last_history_entries_before(timeline_id, graph_id, &missing, task)
            .await?
        {
            last_kept.insert(node_name, decode_values(&values)?);
        }
        Ok(())
    }

    #[task]
    async fn mark_empty_frames(
        &self,
        timeline_id: &TimelineID,
        frames: &[FrameRow],
        status_by_id: &BTreeMap<GraphID, HistoryStatusRow>,
        report: &mut HistoryIngestReport,
        task: &ll::Task,
    ) -> Result<()> {
        let rows = frames
            .iter()
            .filter(|frame| frame.frame_type == FrameType::Empty)
            .filter(|frame| {
                status_by_id
                    .get(&frame.frame.graph_id)
                    .and_then(|row| row.status.parse::<HistoryStatus>().ok())
                    != Some(HistoryStatus::Empty)
            })
            .map(|frame| HistoryStatusRow {
                graph_id: frame.frame.graph_id,
                status: HistoryStatus::Empty.to_string(),
                attempts: status_by_id
                    .get(&frame.frame.graph_id)
                    .map_or(0, |row| row.attempts),
                error_blob_key: None,
                omission_deferred: false,
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Ok(());
        }

        // Chunked: a mirrored timeline can hold tens of thousands of unfilled
        // placeholders, and one transaction over all of them would hold the
        // timeline's exclusive lock long enough to stall graph ingestion.
        let total = i64::try_from(rows.len())?;
        task.data("placeholders", total);
        task.progress(0, total);

        for chunk in rows.chunks(STATUS_CHUNK_SIZE) {
            let mut conn = self.ctx.storage.graph.conn_write().await?;
            conn.start_transaction(&task).await?;
            conn.get_timeline_config_and_lock(timeline_id, &task)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
            conn.upsert_history_status(timeline_id, chunk, &task)
                .await?;
            conn.commit_transaction(&task).await?;
            report.empty += chunk.len();
            task.progress(i64::try_from(report.empty)?, total);
        }
        Ok(())
    }

    /// Write one frame's rows and its checkpoint in a single transaction.
    ///
    /// Anchors ride along with the frame that needed them, at the *previous*
    /// frame's graph ID. Committing them together is what guarantees a sample
    /// is never left without the row that explains it.
    async fn commit_processed_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        entries: FrameEntries,
        deferred: bool,
        existing_status: Option<&HistoryStatusRow>,
        task: &ll::Task,
    ) -> Result<CommitSummary> {
        let status = if entries.samples == 0 {
            HistoryStatus::Omitted
        } else {
            HistoryStatus::Processed
        };
        let old_error_key = existing_status
            .and_then(|status| status.error_blob_key.clone())
            .into_iter()
            .collect::<Vec<_>>();
        let attempts = existing_status.map_or(0, |status| status.attempts);

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.insert_history_entries(timeline_id, &entries.rows, task)
            .await?;
        conn.upsert_history_status(
            timeline_id,
            &[HistoryStatusRow {
                graph_id: frame.frame.graph_id,
                status: status.to_string(),
                attempts,
                error_blob_key: None,
                omission_deferred: deferred,
            }],
            task,
        )
        .await?;
        conn.register_blobs_for_cleanup(&old_error_key, task)
            .await?;
        conn.commit_transaction(task).await?;

        Ok(CommitSummary {
            status,
            entries: entries.samples,
            deferred_rows: entries.deferred_rows,
            anchors: entries.anchors,
            deferred,
        })
    }

    async fn record_error(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        existing_status: Option<&HistoryStatusRow>,
        error: anyhow::Error,
        task: &ll::Task,
    ) -> Result<()> {
        let attempts = existing_status.map_or(0, |status| status.attempts) + 1;
        let blob_key = format!("{}/{}/history_error", timeline_id.0, frame.frame.graph_id.0);
        self.put_error_blob(&blob_key, existing_status, &error, task)
            .await?;

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.upsert_history_status(
            timeline_id,
            &[HistoryStatusRow {
                graph_id: frame.frame.graph_id,
                status: HistoryStatus::Error.to_string(),
                attempts,
                error_blob_key: Some(blob_key.clone()),
                omission_deferred: false,
            }],
            task,
        )
        .await?;
        conn.unregister_blobs_for_cleanup(&[blob_key], task).await?;
        conn.commit_transaction(task).await
    }

    async fn put_error_blob(
        &self,
        blob_key: &str,
        existing_status: Option<&HistoryStatusRow>,
        error: &anyhow::Error,
        task: &ll::Task,
    ) -> Result<()> {
        let payload = ErrorPayload {
            messages: vec![error.to_string()],
            details: Some(format!("{error:#}")),
        };
        let data = serde_json::to_vec(&payload).context("failed to serialize ErrorPayload")?;
        if existing_status.and_then(|status| status.error_blob_key.as_deref()) != Some(blob_key) {
            let mut conn = self.ctx.storage.graph.conn_write().await?;
            conn.register_blobs_for_cleanup(&[blob_key.to_string()], task)
                .await?;
        }
        self.ctx.storage.blob.put_blob(blob_key, &data).await
    }

    async fn compact_node(
        &self,
        timeline_id: &TimelineID,
        node_name: &str,
        threshold: f64,
        range: &HistoryRange,
        frames: &[GraphID],
        task: &ll::Task,
    ) -> Result<CompactCounts> {
        let mut conn = self.ctx.storage.graph.conn_analytics().await?;
        let rows = conn
            .get_history_series(timeline_id, node_name, range, task)
            .await?;
        drop(conn);

        let series = rows
            .into_iter()
            .map(|row| {
                Ok(CompactRow {
                    graph_id: row.graph_id,
                    values: decode_values(&row.values)?,
                    anchor: row.anchor,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let seed = self
            .seed_before(timeline_id, node_name, range, task)
            .await?;
        let plan = compact_series(&CompactInput {
            series: &series,
            seed: seed.as_ref(),
            frames,
            threshold,
        });
        if plan.dropped.is_empty() && plan.anchored.is_empty() {
            return Ok(CompactCounts::default());
        }

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.delete_history_entries_for_node(timeline_id, node_name, &plan.dropped, task)
            .await?;
        // One statement per promoted row: they are keyed by graph ID, and a
        // node has at most one per surviving sample.
        let node = [node_name.to_owned()];
        for graph_id in &plan.anchored {
            conn.set_history_entries_anchor_at(timeline_id, *graph_id, &node, task)
                .await?;
        }
        conn.commit_transaction(task).await?;
        Ok(CompactCounts {
            dropped: plan.dropped.len(),
            anchored: plan.anchored.len(),
        })
    }

    /// The range's built frames, ascending.
    ///
    /// Compaction needs frame adjacency, not row adjacency: the row before a
    /// sample in the series can be thousands of frames back, and only the
    /// immediately preceding *frame* can anchor it. Read once per range and
    /// shared across every node in it.
    async fn built_frames_in_range(
        &self,
        timeline_id: &TimelineID,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<Vec<GraphID>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let rows = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: timeline_id.clone(),
                    limit: None,
                    frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                    order: Some(Order::Asc),
                    timestamp_bounds: Some(range.timestamps.clone()),
                    graph_id_bounds: Some(range.graph_ids),
                    graph_ids: None,
                    with_data: Some(false),
                    before: None,
                    expires_before: None,
                },
                task,
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.frame.graph_id)
            .collect::<Vec<_>>())
    }

    /// Reconsider just the rows ingest could not judge, one flagged frame at a
    /// time.
    ///
    /// Scales with flagged frames rather than with the timeline's node count,
    /// which is the whole point — a scheduled run typically has a handful of
    /// frames to settle, against hundreds of thousands of nodes.
    ///
    /// Ascending order is load-bearing: dropping a frame's rows moves the
    /// baseline the next frame's rows are measured against. Each frame commits
    /// on its own, so an interrupted run keeps the frames it already finished.
    async fn compact_deferred_frames(
        &self,
        timeline_id: &TimelineID,
        threshold: f64,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<HistoryCompactReport> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let graph_ids = conn
            .list_history_deferred_graph_ids(timeline_id, &range.graph_ids, task)
            .await?;
        drop(conn);

        let mut report = HistoryCompactReport {
            nodes: 0,
            dropped: 0,
            anchored: 0,
            compacted_through: range.graph_ids.1,
        };
        let total = i64::try_from(graph_ids.len())?;
        task.data("deferred_frames", total);
        task.progress(0, total);

        for (index, graph_id) in graph_ids.into_iter().enumerate() {
            let frame = self
                .compact_deferred_frame(timeline_id, graph_id, threshold, task)
                .await?;
            report.nodes += frame.nodes;
            report.dropped += frame.dropped;
            report.anchored += frame.anchored;
            task.progress(i64::try_from(index)? + 1, total);
        }
        Ok(report)
    }

    /// Judge one flagged frame's deferred rows against their current baselines.
    ///
    /// The baseline is whatever survives before this frame — which is exactly
    /// what may have changed since ingest deferred these rows, because the
    /// frame that filled the hole has landed in the meantime.
    ///
    /// A row that loses is not always deleted: if the next built frame kept a
    /// row for the same node, this one stays as that sample's anchor. Anchors
    /// already here are left alone entirely — judging one would always drop it.
    async fn compact_deferred_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        threshold: f64,
        task: &ll::Task,
    ) -> Result<HistoryCompactReport> {
        let mut conn = self.ctx.storage.graph.conn_analytics().await?;
        let candidates = conn
            .get_history_entries_at(timeline_id, graph_id, task)
            .await?;
        let node_names = candidates
            .iter()
            .map(|candidate| candidate.node_name.clone())
            .collect::<Vec<_>>();
        let baselines = conn
            .get_last_history_entries_before(timeline_id, graph_id, &node_names, task)
            .await?
            .into_iter()
            .map(|(node_name, values)| Ok((node_name, decode_values(&values)?)))
            .collect::<Result<HashMap<_, _>>>()?;
        drop(conn);

        let successors = self
            .rows_at_next_built_frame(timeline_id, graph_id, task)
            .await?;

        let mut dropped = Vec::new();
        let mut anchored = Vec::new();
        for candidate in &candidates {
            if candidate.anchor {
                continue;
            }
            let baseline = baselines.get(&candidate.node_name);
            let values = decode_values(&candidate.values)?;
            if keep_row(baseline, &values, threshold) {
                continue;
            }
            // Redundant. It only earns its keep if the frame right after it
            // holds a sample that *survives* — judged against this same
            // baseline, since neither a dropped row nor an anchor becomes one.
            match successors.get(&candidate.node_name) {
                Some(next) if keep_row(baseline, next, threshold) => {
                    anchored.push(candidate.node_name.clone());
                }
                _ => dropped.push(candidate.node_name.clone()),
            }
        }

        self.commit_compacted_frame(timeline_id, graph_id, &dropped, &anchored, task)
            .await?;
        Ok(HistoryCompactReport {
            nodes: candidates.len(),
            dropped: dropped.len(),
            anchored: anchored.len(),
            compacted_through: Some(graph_id),
        })
    }

    /// Per node, the values recorded at the built frame right after `graph_id`.
    ///
    /// What a redundant row at `graph_id` might be kept to explain. Frames are
    /// compacted in ascending order, so the successor has not been judged yet —
    /// hence the values rather than just the node names: whether it survives is
    /// something the caller has to work out for itself, and getting that wrong
    /// means anchoring every row in a run of redundant frames.
    ///
    /// Existing anchors are excluded. An anchor is not a sample, so it is not
    /// something another row needs to explain.
    async fn rows_at_next_built_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<HashMap<String, BTreeMap<u32, f64>>> {
        let Some(next) = self.next_built_frame(timeline_id, graph_id, task).await? else {
            return Ok(HashMap::new());
        };
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.get_history_entries_at(timeline_id, next.frame.graph_id, task)
            .await?
            .into_iter()
            .filter(|sample| !sample.anchor)
            .map(|sample| Ok((sample.node_name, decode_values(&sample.values)?)))
            .collect()
    }

    /// Delete the frame's redundant rows, keep the ones still explaining
    /// something, and retire its deferral bookkeeping.
    ///
    /// Whatever deferred rows remain after this survived the threshold, so
    /// clearing the flag on the frame promotes them to baselines — which later
    /// frames in this same walk will then measure against. Anchors keep their
    /// own flag and stay out of baseline lookups.
    async fn commit_compacted_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        dropped: &[String],
        anchored: &[String],
        task: &ll::Task,
    ) -> Result<()> {
        let bounds = (Some(graph_id), Some(graph_id));
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.delete_history_entries_at(timeline_id, graph_id, dropped, task)
            .await?;
        conn.set_history_entries_anchor_at(timeline_id, graph_id, anchored, task)
            .await?;
        conn.clear_history_entries_deferred(timeline_id, &bounds, task)
            .await?;
        conn.clear_history_omission_deferred(timeline_id, &bounds, task)
            .await?;
        conn.commit_transaction(task).await
    }

    /// Re-derive every node's whole series in the range.
    ///
    /// The fallback for a genuine threshold change, which can invalidate rows
    /// that were never flagged. Costs one series read plus one transaction per
    /// node, so keep the range tight.
    async fn compact_every_node(
        &self,
        timeline_id: &TimelineID,
        threshold: f64,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<HistoryCompactReport> {
        let mut conn = self.ctx.storage.graph.conn_analytics().await?;
        let node_names = conn
            .list_history_node_names(timeline_id, range, task)
            .await?;
        drop(conn);

        let frames = self.built_frames_in_range(timeline_id, range, task).await?;

        let mut report = HistoryCompactReport {
            nodes: node_names.len(),
            dropped: 0,
            anchored: 0,
            compacted_through: range.graph_ids.1,
        };
        let total = i64::try_from(node_names.len())?;
        task.data("nodes_in_range", total);
        task.progress(0, total);

        for (index, node_name) in node_names.into_iter().enumerate() {
            let counts = self
                .compact_node(timeline_id, &node_name, threshold, range, &frames, task)
                .await?;
            report.dropped += counts.dropped;
            report.anchored += counts.anchored;
            task.progress(i64::try_from(index)? + 1, total);
        }

        self.clear_deferred(timeline_id, &range.graph_ids, task)
            .await?;
        Ok(report)
    }

    /// The node's last kept sample strictly before the compaction range.
    ///
    /// Without it the range's first row would compare against nothing and
    /// always survive, so compacting in windows would keep more rows than one
    /// whole-timeline pass. An unbounded range needs no seed — its first row
    /// genuinely is the node's first sample.
    async fn seed_before(
        &self,
        timeline_id: &TimelineID,
        node_name: &str,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<Option<BTreeMap<u32, f64>>> {
        let Some(lower) = range.graph_ids.0 else {
            return Ok(None);
        };
        let mut conn = self.ctx.storage.graph.conn().await?;
        let entries = conn
            .get_last_history_entries_before(
                timeline_id,
                lower,
                std::slice::from_ref(&node_name.to_string()),
                task,
            )
            .await?;
        entries
            .first()
            .map(|(_node_name, values)| decode_values(values))
            .transpose()
    }

    /// Delete all of a timeline's recorded history, including its metric-name
    /// dictionary. The timeline itself and its frames are untouched — this
    /// only clears the `graph_history_*` tables.
    ///
    /// Unbounded history can reach ~1e9 rows, and a single `DELETE` that large
    /// is a real SQLite hazard, so the work is split into `DELETE_CHUNK_SIZE`
    /// graph-ID ranges, each its own transaction (see [`Self::delete_bounded`]).
    /// Partial progress is safe and the whole thing is re-runnable: error blobs
    /// are only *registered* for cleanup, and the sweeper won't touch them
    /// until they age past its `min_age` window.
    ///
    /// The chunk range comes from the timeline's frames rather than the entries
    /// themselves, so the final unbounded [`Self::delete_metrics`] also acts as
    /// the backstop for any entry whose frame has since been deleted.
    async fn delete_all_history(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<HistoryDeleteReport> {
        let frames = self
            .select_history_frames(timeline_id, TimestampBounds::default(), (None, None), task)
            .await?;
        // No frames left to chunk over — fall back to one unbounded delete.
        let Some(first) = frames.first() else {
            return self
                .delete_bounded(timeline_id, &(None, None), true, task)
                .await;
        };
        let mut report = HistoryDeleteReport::default();
        let mut start = first.frame.graph_id.0;
        let end = frames
            .last()
            .map(|frame| frame.frame.graph_id.0)
            .unwrap_or(first.frame.graph_id.0);

        // Chunks span graph-ID space, which is sparse, so this counts ranges
        // swept rather than rows deleted — steady progress, not a row estimate.
        let total = (end - start).div_euclid(DELETE_CHUNK_SIZE) + 1;
        task.data("delete_chunks", total);
        task.progress(0, total);
        let mut done = 0i64;

        while start <= end {
            let upper = end.min(start + DELETE_CHUNK_SIZE - 1);
            let chunk = self
                .delete_bounded(
                    timeline_id,
                    &(Some(GraphID(start)), Some(GraphID(upper))),
                    false,
                    task,
                )
                .await?;
            report.entries_deleted += chunk.entries_deleted;
            report.statuses_deleted += chunk.statuses_deleted;
            report.error_blobs_registered += chunk.error_blobs_registered;
            start = upper + 1;
            done += 1;
            task.progress(done, total);
        }
        let metrics = self.delete_metrics(timeline_id, task).await?;
        report.metrics_deleted = metrics;
        Ok(report)
    }

    /// Delete history entries and checkpoints within `bounds` in one
    /// transaction, registering any error blobs in the range for cleanup
    /// first so the registration rolls back with the delete if it fails.
    ///
    /// `delete_metrics` also drops the metric-name dictionary. Only pass
    /// `true` when nothing else in the timeline can still reference those ids
    /// — a surviving entry would decode against the wrong names.
    async fn delete_bounded(
        &self,
        timeline_id: &TimelineID,
        bounds: &(Option<GraphID>, Option<GraphID>),
        delete_metrics: bool,
        task: &ll::Task,
    ) -> Result<HistoryDeleteReport> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        let error_blob_keys = conn
            .get_history_error_blob_keys(timeline_id, bounds, task)
            .await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.register_blobs_for_cleanup(&error_blob_keys, task)
            .await?;
        let entries_deleted = conn
            .delete_history_entries(timeline_id, bounds, task)
            .await?;
        let statuses_deleted = conn
            .delete_history_status(timeline_id, bounds, task)
            .await?;
        let metrics_deleted = if delete_metrics {
            conn.delete_history_metrics(timeline_id, task).await?
        } else {
            0
        };
        conn.commit_transaction(task).await?;
        Ok(HistoryDeleteReport {
            entries_deleted,
            statuses_deleted,
            metrics_deleted,
            error_blobs_registered: error_blob_keys.len(),
        })
    }

    async fn delete_metrics(&self, timeline_id: &TimelineID, task: &ll::Task) -> Result<u64> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        let deleted = conn.delete_history_metrics(timeline_id, task).await?;
        conn.commit_transaction(task).await?;
        Ok(deleted)
    }

    async fn load_statuses(
        &self,
        timeline_id: &TimelineID,
        frames: &[FrameRow],
        task: &ll::Task,
    ) -> Result<BTreeMap<GraphID, HistoryStatusRow>> {
        let graph_ids = frames
            .iter()
            .map(|frame| frame.frame.graph_id)
            .collect::<Vec<_>>();
        let mut conn = self.ctx.storage.graph.conn().await?;
        Ok(conn
            .get_history_status(timeline_id, &graph_ids, task)
            .await?
            .into_iter()
            .map(|row| (row.graph_id, row))
            .collect())
    }
}

enum FrameAction {
    Process,
    Skip,
}

/// Does this frame still need ingesting?
///
/// The `Empty` checkpoint case is the subtle one. Placeholders get stamped
/// `Empty` so a scheduled run does not reconsider them, but a placeholder is
/// exactly the thing that later becomes a real frame. Treating that stamp as
/// final would permanently blacklist every frame that happened to be unfilled
/// when history first swept past it — on a mirrored timeline, most of them.
/// So the stamp is keyed off the frame's *current* type, not its checkpoint.
fn frame_action(frame: &FrameRow, status: Option<&HistoryStatusRow>) -> FrameAction {
    // Empty carries no metrics yet; Error never will.
    if matches!(frame.frame_type, FrameType::Empty | FrameType::Error) {
        return FrameAction::Skip;
    }
    let Some(status) = status else {
        return FrameAction::Process;
    };
    match status.status.parse::<HistoryStatus>() {
        Ok(HistoryStatus::Processed | HistoryStatus::Omitted) => FrameAction::Skip,
        Ok(HistoryStatus::Empty) => FrameAction::Process,
        Ok(HistoryStatus::Error) if status.attempts >= i64::from(MAX_ATTEMPTS) => FrameAction::Skip,
        Ok(HistoryStatus::Error) => FrameAction::Process,
        Err(_) => FrameAction::Skip,
    }
}

/// Can the *next* frame trust a verdict measured against this one?
///
/// Two independent ways the answer is no, and both matter:
///
/// - The frame can still change — see [`is_frame_settled`]. A frame appearing
///   behind us would invalidate anything measured across the gap.
/// - The frame's own verdicts are still flagged. A settled predecessor only
///   rules out *new* frames; it says nothing about whether the row we would
///   measure against survives. Rows at a flagged frame are provisional, so
///   anything judged against one is provisional too. Provisionality therefore
///   propagates forward until `compact` retires it, frame by frame.
fn is_frame_final(
    frame: &FrameRow,
    status: Option<&HistoryStatusRow>,
    settle_cutoff: Timestamp,
) -> bool {
    is_frame_settled(
        &frame.frame_type,
        frame.frame.timestamp,
        status,
        settle_cutoff,
    ) && !status.is_some_and(|status| status.omission_deferred)
}

/// Timestamp before which an unfilled frame is presumed abandoned.
fn settle_cutoff(settle_hours: usize) -> Result<Timestamp> {
    Timestamp::now()
        .subtract_hours(settle_hours)
        .context("settle_hours is too large")
}

/// Decide which of a frame's rows to write, and mint the anchors they need.
///
/// Advances `last_kept` for every row that cleared the threshold — and only
/// those. A deferred row is provisional (compaction will delete it) and an
/// anchor never cleared the bar to begin with, so letting either become the
/// baseline would measure the next sample against a value that hides the drift
/// accumulated since the last *surviving* sample. That next sample would then
/// be omitted, and omission is permanent.
fn threshold_frame(
    frame: &FrameRow,
    current_rows: &NodeValues,
    prev_frame: Option<&PrevFrame>,
    last_kept: &mut HashMap<String, BTreeMap<u32, f64>>,
    threshold: f64,
    omissible: bool,
) -> FrameEntries {
    let mut rows = Vec::new();
    let mut kept = Vec::new();
    let mut deferred_rows = 0usize;

    for (node_name, current) in current_rows {
        let above_threshold = keep_row(last_kept.get(node_name), current, threshold);
        if !above_threshold && omissible {
            continue;
        }
        rows.push(HistoryEntryRow {
            node_name: node_name.clone(),
            graph_id: frame.frame.graph_id,
            timestamp: frame.frame.timestamp,
            values: encode_values(current),
            deferred: !above_threshold,
            anchor: false,
        });

        if above_threshold {
            last_kept.insert(node_name.clone(), current.clone());
            kept.push(node_name);
        } else {
            deferred_rows += 1;
        }
    }

    let samples = rows.len();
    let anchors = mint_anchors(prev_frame, &kept);
    let anchor_count = anchors.len();
    rows.extend(anchors);

    FrameEntries {
        rows,
        samples,
        deferred_rows,
        anchors: anchor_count,
    }
}

/// Rows recording what the previous frame held for the nodes kept at this one.
///
/// Without them a kept sample's step reads as all the drift since the node's
/// last kept row — hundreds of folded-away diffs — rather than the contribution
/// of the one graph that crossed the threshold.
///
/// Nodes the previous frame knew nothing about are skipped: their sample here
/// is the node's first, which already reads correctly, and an anchor of zeros
/// for every newly appearing node would double the cost of a growing graph for
/// nothing.
fn mint_anchors(prev_frame: Option<&PrevFrame>, kept: &[&String]) -> Vec<HistoryEntryRow> {
    let Some(prev) = prev_frame else {
        return Vec::new();
    };
    kept.iter()
        .filter(|node_name| !prev.has_row.contains(**node_name))
        .filter_map(|node_name| {
            let values = prev.values.get(*node_name)?;
            Some(HistoryEntryRow {
                node_name: (*node_name).clone(),
                graph_id: prev.graph_id,
                timestamp: prev.timestamp,
                values: encode_values(values),
                deferred: false,
                anchor: true,
            })
        })
        .collect()
}

fn apply_commit_summary(report: &mut HistoryIngestReport, summary: CommitSummary) {
    report.entries += summary.entries;
    report.deferred_rows += summary.deferred_rows;
    report.anchors += summary.anchors;
    if summary.deferred {
        report.deferred += 1;
    }
    match summary.status {
        HistoryStatus::Processed => report.processed += 1,
        HistoryStatus::Omitted => report.omitted += 1,
        HistoryStatus::Error | HistoryStatus::Empty => {}
    }
}

fn metric_snapshots_to_ids(
    extracted: &NodeMetrics,
    metric_ids: &BTreeMap<String, u32>,
) -> Result<BTreeMap<String, BTreeMap<u32, f64>>> {
    extracted
        .iter()
        .map(|(node_name, metrics)| {
            let values = metrics
                .iter()
                .map(|(metric_name, value)| {
                    let metric_id = metric_ids
                        .get(metric_name)
                        .ok_or_else(|| anyhow::anyhow!("metric was not interned: {metric_name}"))?;
                    Ok((*metric_id, *value))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            Ok((node_name.clone(), values))
        })
        .collect()
}

fn decode_named_values(
    metric_names: &BTreeMap<u32, String>,
    values: &[u8],
) -> Result<BTreeMap<String, f64>> {
    decode_values(values)?
        .into_iter()
        .map(|(metric_id, value)| {
            let name = metric_names
                .get(&metric_id)
                .ok_or_else(|| anyhow::anyhow!("missing metric name for id {metric_id}"))?;
            Ok((name.clone(), value))
        })
        .collect()
}

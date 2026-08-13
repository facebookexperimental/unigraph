// Copyright (c) Meta Platforms, Inc. and affiliates.

//! The frame sequence a run works against, read once and recomputed from
//! scratch.
//!
//! Gap structure is derived from the frame types every run rather than trusted
//! from storage, so a bookkeeping error cannot outlive a single pass. The two
//! reads that costs — every frame's metadata and every checkpoint — are narrow
//! rows, and the design this replaced already paid the same price on every
//! `compact` to locate its frontier.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
use ll::task;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::HistoryStatusRow;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use super::GraphHistory;
use super::HistoryIngestOptions;
use super::HistoryIngestReport;
use crate::graph_history::FrameFlags;
use crate::graph_history::FrameGap;
use crate::graph_history::IngestState;
use crate::graph_history::MAX_ATTEMPTS;
use crate::graph_history::desired_flags;
use crate::graph_history::frame_has_data;
use crate::graph_history::reconcile_flags;

/// Max status rows written per transaction when seeding checkpoints.
///
/// A timeline can hold tens of thousands of unfilled placeholders, and every
/// status write takes the timeline's exclusive lock. Chunking keeps a single
/// ingest run from blocking the graph pipeline behind one huge transaction.
const STATUS_CHUNK_SIZE: usize = 2_000;

/// Everything a run needs to know about the frame sequence.
pub(super) struct FrameContext {
    /// Every frame in the timeline, ascending, metadata only.
    pub(super) frames: Vec<FrameRow>,
    /// The checkpoint for each frame.
    pub(super) statuses: BTreeMap<GraphID, HistoryStatusRow>,
    /// What each frame's gap flags *should* be, given the sequence.
    pub(super) flags: BTreeMap<GraphID, FrameFlags>,
    /// Frames that carry metric values, ascending.
    pub(super) data_frames: Vec<GraphID>,
}

/// What a checkpoint sync changed.
pub(super) struct SyncCounts {
    pub(super) flags_updated: usize,
    pub(super) rejudged: usize,
}

/// What a run should do with one frame.
enum FrameAction {
    /// Judge it against the frame before it.
    Ingest,
    /// It carries no values. Nothing to do but leave it on the work list.
    NoData,
    /// Already judged, past the attempt cap, or out of range.
    Skip,
}

impl FrameContext {
    pub(super) fn state(&self, graph_id: GraphID) -> Option<IngestState> {
        self.statuses
            .get(&graph_id)
            .map(|status| status.ingest_state)
    }

    pub(super) fn attempts(&self, graph_id: GraphID) -> i64 {
        self.statuses
            .get(&graph_id)
            .map_or(0, |status| status.attempts)
    }

    pub(super) fn error_blob_key(&self, graph_id: GraphID) -> Option<String> {
        self.statuses
            .get(&graph_id)
            .and_then(|status| status.error_blob_key.clone())
    }

    pub(super) fn flags(&self, graph_id: GraphID) -> FrameFlags {
        self.flags.get(&graph_id).copied().unwrap_or_default()
    }

    /// When the frame was registered. Needed to stamp a row written for a
    /// predecessor this run had to go back and fetch.
    pub(super) fn timestamp_of(&self, graph_id: GraphID) -> Option<Timestamp> {
        let index = self
            .frames
            .binary_search_by_key(&graph_id, |frame| frame.frame.graph_id)
            .ok()?;
        Some(self.frames[index].frame.timestamp)
    }

    /// The data frame immediately before `graph_id`, if the timeline holds one.
    pub(super) fn preceding_data_frame(&self, graph_id: GraphID) -> Option<GraphID> {
        let index = self.data_frames.partition_point(|id| *id < graph_id);
        self.data_frames.get(index.checked_sub(1)?).copied()
    }

    /// The newest frame that carries values — where the `LATEST` pin belongs.
    pub(super) fn newest_data_frame(&self) -> Option<GraphID> {
        self.data_frames.last().copied()
    }

    /// The newest data frame already judged.
    ///
    /// Where the `LATEST` pin currently sits, because every run leaves it on
    /// the newest frame it ingested. Derived rather than stored: one more
    /// column would be one more thing that can disagree with reality.
    pub(super) fn latest_ingested(&self) -> Option<GraphID> {
        self.data_frames
            .iter()
            .rev()
            .find(|graph_id| self.state(**graph_id) == Some(IngestState::Ingested))
            .copied()
    }

    /// The gap picture as a list, in the shape the pure functions want.
    pub(super) fn gaps(&self) -> Vec<FrameGap> {
        frame_gaps(&self.frames, &self.statuses)
    }

    /// Frames still needing work, ascending, honouring the caller's bounds.
    pub(super) fn work_list(
        &self,
        options: &HistoryIngestOptions,
        report: &mut HistoryIngestReport,
    ) -> Result<Vec<FrameRow>> {
        let cutoff = options
            .lookback_hours
            .map(|hours| {
                Timestamp::now()
                    .subtract_hours(hours)
                    .context("lookback_hours is too large")
            })
            .transpose()?;

        let mut work = Vec::new();
        for frame in &self.frames {
            if !within_bounds(frame.frame.graph_id, &options.graph_id_bounds) {
                report.skipped += 1;
                continue;
            }
            match self.action(frame) {
                FrameAction::NoData => report.no_data += 1,
                FrameAction::Skip => report.skipped += 1,
                FrameAction::Ingest => match cutoff {
                    Some(cutoff) if frame.frame.timestamp < cutoff => report.skipped += 1,
                    _ => work.push(frame.clone()),
                },
            }
        }
        Ok(work)
    }

    fn action(&self, frame: &FrameRow) -> FrameAction {
        let graph_id = frame.frame.graph_id;
        // A placeholder or a failed build carries nothing to judge. It stays on
        // the work list forever anyway — that is exactly the frame that later
        // turns into a real one.
        if matches!(frame.frame_type, FrameType::Empty | FrameType::Error) {
            return FrameAction::NoData;
        }
        match self.state(graph_id) {
            Some(state) if !state.needs_ingest(self.attempts(graph_id), MAX_ATTEMPTS) => {
                FrameAction::Skip
            }
            _ => FrameAction::Ingest,
        }
    }
}

/// Is `graph_id` inside the caller's inclusive bounds?
pub(super) fn within_bounds(graph_id: GraphID, bounds: &GraphIDBounds) -> bool {
    bounds.0.is_none_or(|from| graph_id >= from) && bounds.1.is_none_or(|to| graph_id <= to)
}

/// Reduce frames plus checkpoints to the gap picture.
fn frame_gaps(
    frames: &[FrameRow],
    statuses: &BTreeMap<GraphID, HistoryStatusRow>,
) -> Vec<FrameGap> {
    frames
        .iter()
        .map(|frame| {
            let graph_id = frame.frame.graph_id;
            let status = statuses.get(&graph_id);
            let state = status.map(|status| status.ingest_state);
            FrameGap {
                graph_id,
                has_data: frame_has_data(&frame.frame_type, state),
                stored: status.map_or(FrameFlags::empty(), |status| status.frame_flags),
            }
        })
        .collect()
}

/// The state a frame's very first checkpoint should hold.
fn initial_state(frame_type: &FrameType) -> IngestState {
    match frame_type {
        FrameType::Empty | FrameType::Error => IngestState::NoData,
        FrameType::Full | FrameType::Delta => IngestState::Pending,
    }
}

impl GraphHistory {
    /// Every frame, every checkpoint, and the gap flags the sequence implies.
    pub(super) async fn load_frame_context(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<FrameContext> {
        let frames = self
            .select_history_frames(timeline_id, TimestampBounds::default(), (None, None), task)
            .await?;

        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.get_timeline_config(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        let statuses = conn
            .list_history_statuses(timeline_id, &(None, None), task)
            .await?
            .into_iter()
            .map(|row| (row.graph_id, row))
            .collect::<BTreeMap<_, _>>();
        drop(conn);

        Ok(build_context(frames, statuses))
    }

    /// Give every frame a checkpoint, and bring the stored gap flags back in
    /// line with the sequence.
    ///
    /// Seeding a checkpoint for every frame is what makes the work list total.
    /// A frame with no row at all is invisible to it, and that is precisely how
    /// the design this replaced lost frames for good: an outage longer than the
    /// lookback window left built frames nothing would ever look at again.
    #[task]
    pub(super) async fn sync_checkpoints(
        &self,
        timeline_id: &TimelineID,
        context: &mut FrameContext,
        task: &ll::Task,
    ) -> Result<SyncCounts> {
        for row in self
            .seed_missing_checkpoints(timeline_id, context, &task)
            .await?
        {
            context.statuses.insert(row.graph_id, row);
        }

        let updates = reconcile_flags(&context.gaps());
        if updates.is_empty() {
            return Ok(SyncCounts {
                flags_updated: 0,
                rejudged: 0,
            });
        }

        let flags = updates
            .iter()
            .map(|update| (update.graph_id, update.flags))
            .collect::<Vec<_>>();
        // A frame that has stopped being a gap's far edge holds rows that were
        // never judged. Judging them needs the predecessor's values, which
        // means a graph — so it goes back on the one work list rather than
        // getting a second mechanism of its own.
        let rejudge = updates
            .iter()
            .filter(|update| update.needs_rejudge)
            .map(|update| update.graph_id)
            .collect::<Vec<_>>();

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(&task).await?;
        conn.get_timeline_config_and_lock(timeline_id, &task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.set_history_frame_flags(timeline_id, &flags, &task)
            .await?;
        conn.set_history_ingest_states(timeline_id, &rejudge, IngestState::Pending, &task)
            .await?;
        conn.commit_transaction(&task).await?;

        for update in &updates {
            let Some(status) = context.statuses.get_mut(&update.graph_id) else {
                continue;
            };
            status.frame_flags = update.flags;
            if update.needs_rejudge {
                status.ingest_state = IngestState::Pending;
            }
        }
        task.data("frame_flags_updated", i64::try_from(updates.len())?);
        Ok(SyncCounts {
            flags_updated: updates.len(),
            rejudged: rejudge.len(),
        })
    }

    /// Write a checkpoint for every frame that has none yet.
    #[task]
    async fn seed_missing_checkpoints(
        &self,
        timeline_id: &TimelineID,
        context: &FrameContext,
        task: &ll::Task,
    ) -> Result<Vec<HistoryStatusRow>> {
        let rows = context
            .frames
            .iter()
            .filter(|frame| !context.statuses.contains_key(&frame.frame.graph_id))
            .map(|frame| HistoryStatusRow {
                graph_id: frame.frame.graph_id,
                ingest_state: initial_state(&frame.frame_type),
                attempts: 0,
                error_blob_key: None,
                frame_flags: context.flags(frame.frame.graph_id),
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Ok(rows);
        }

        let total = i64::try_from(rows.len())?;
        task.data("new_checkpoints", total);
        task.progress(0, total);
        let mut done = 0i64;

        for chunk in rows.chunks(STATUS_CHUNK_SIZE) {
            let mut conn = self.ctx.storage.graph.conn_write().await?;
            conn.start_transaction(&task).await?;
            conn.get_timeline_config_and_lock(timeline_id, &task)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
            conn.upsert_history_status(timeline_id, chunk, &task)
                .await?;
            conn.commit_transaction(&task).await?;
            done += i64::try_from(chunk.len())?;
            task.progress(done, total);
        }
        Ok(rows)
    }
}

/// Assemble the derived views once, so no caller recomputes them.
fn build_context(
    frames: Vec<FrameRow>,
    statuses: BTreeMap<GraphID, HistoryStatusRow>,
) -> FrameContext {
    let gaps = frame_gaps(&frames, &statuses);
    let flags = gaps
        .iter()
        .map(|gap| gap.graph_id)
        .zip(desired_flags(&gaps))
        .collect::<BTreeMap<_, _>>();
    let data_frames = gaps
        .iter()
        .filter(|gap| gap.has_data)
        .map(|gap| gap.graph_id)
        .collect();

    FrameContext {
        frames,
        statuses,
        flags,
        data_frames,
    }
}

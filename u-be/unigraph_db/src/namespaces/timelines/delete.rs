// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Deleting a timeline and everything stored under it.
//!
//! ```text
//! frames        batched, one transaction each   -> registers external blobs
//! history       reused from graph_history       -> registers error blobs
//! metric history  one transaction per ISO week
//! external ids  one statement, if unshared
//! config row    last — every delete above locks it
//! blob sweep    physically deletes what has aged past `sweep_min_age`
//! ```
//!
//! # Why batched
//!
//! A timeline can hold ~1e9 frames. One `DELETE` that large is a real hazard on
//! either backend, and frame deletion is not a plain row delete: each frame's
//! manifest has to be read first to find the external blobs it references, and
//! those keys registered for cleanup *in the same transaction* as the delete, so
//! a rollback can never leave a live blob scheduled for deletion. So the pass
//! walks the timeline in `batch_size` chunks, each its own transaction, each
//! holding the timeline lock for as long as it takes to delete one chunk and no
//! longer. Partial progress is safe and the whole thing is re-runnable — the
//! config row goes last, and until it does every step above can still find it.
//!
//! # Why the blobs are not gone when this returns
//!
//! Deleting a frame never physically deletes its external blobs; it registers
//! their keys in `blobs_to_delete` and lets [`sweep_blobs`] remove them once
//! they are older than `min_age`. That window is load-bearing and global — the
//! cleanup table is not per-timeline, and the crash-safe store path registers a
//! blob *before* uploading it, unregistering only on commit. Sweeping a key
//! younger than the longest in-flight store could therefore delete a blob some
//! *other* timeline's committing transaction is about to reference. So the
//! closing sweep here clears the aged backlog and deliberately leaves this run's
//! own registrations to the next one. See `unigraph_db`'s "Blob sweeping and the
//! `min_age` delay".
//!
//! [`sweep_blobs`]: crate::storage::UnigraphStorage::sweep_blobs

use std::time::Duration;

use anyhow::Result;
use ll::task;
use unigraph_storage_core::ExternalIDNamespace;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;

use super::Timelines;
use crate::frame_storage::external_blob_keys;
use crate::namespaces::GraphHistory;
use crate::namespaces::HistoryDeleteReport;
use crate::namespaces::progress::Throughput;
use crate::namespaces::progress::chunks;

/// Frames deleted per transaction when nothing else is specified.
///
/// Sized so one batch is a few seconds of work rather than a few minutes: large
/// enough that a big timeline doesn't turn into millions of round trips, small
/// enough that the timeline lock is never held long and a kill mid-run costs at
/// most one chunk of progress.
pub const DEFAULT_DELETE_BATCH_SIZE: i64 = 10_000;

/// Default `min_age` for the sweep that closes a timeline delete.
///
/// Matches the piggybacked sweep on `graph.delete`: well past any commit or
/// store latency, so the sweep can never race a transaction that is still
/// deciding whether it needs the blob.
pub const DEFAULT_DELETE_SWEEP_MIN_AGE: Duration = Duration::from_secs(2 * 60 * 60);

/// How [`Timelines::delete`] should pace itself.
#[derive(Debug, Clone)]
pub struct TimelineDeleteOptions {
    /// Frames to delete per transaction. Must be positive.
    pub batch_size: i64,
    /// How long a blob must have been pending cleanup before the closing sweep
    /// will physically delete it. `Duration::ZERO` is for tests only — see the
    /// module docs.
    pub sweep_min_age: Duration,
}

impl Default for TimelineDeleteOptions {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_DELETE_BATCH_SIZE,
            sweep_min_age: DEFAULT_DELETE_SWEEP_MIN_AGE,
        }
    }
}

/// What [`Timelines::delete`] removed.
#[derive(Debug, Clone, Default)]
pub struct TimelineDeleteReport {
    pub frames_deleted: u64,
    /// Transactions the frame pass took. A rough measure of how much of the
    /// timeline was left when the run started.
    pub frame_batches: u64,
    /// External blob keys handed to the cleanup table. Not the number swept.
    pub blobs_registered: u64,
    pub history: HistoryDeleteReport,
    pub metric_history_deleted: u64,
    pub external_ids_deleted: u64,
    /// Set when the timeline's external ID namespace was left alone because
    /// another timeline still declares it. Its mappings are that timeline's.
    pub external_id_namespace_shared_with: Option<TimelineID>,
    /// Blobs physically deleted by the closing sweep. Counts the aged backlog,
    /// not this run's registrations — those are still too young.
    pub blobs_swept: usize,
}

impl Timelines {
    /// Delete a timeline and everything stored under it.
    ///
    /// Irreversible, and scoped to one timeline: frames, recorded history,
    /// metric history, the external ID mappings for its namespace (unless
    /// another timeline shares it), and finally the config row itself.
    ///
    /// Traversal and graph-query configs are left alone — they are keyed by
    /// content hash rather than by timeline, shared across timelines, and
    /// already expire on their own TTL.
    ///
    /// Fails if the timeline does not exist. Safe to re-run after a failure
    /// part-way through: every step is idempotent and the config row, which the
    /// earlier steps need, is deleted last.
    #[task]
    pub async fn delete(
        &self,
        timeline_id: &TimelineID,
        options: &TimelineDeleteOptions,
        task: &ll::Task,
    ) -> Result<TimelineDeleteReport> {
        anyhow::ensure!(
            options.batch_size > 0,
            "batch_size must be positive, got {}",
            options.batch_size
        );
        let config = self.require_config(timeline_id, &task).await?;

        let mut report = TimelineDeleteReport::default();
        let frames = self.delete_all_frames(timeline_id, options, &task).await?;
        report.frames_deleted = frames.frames_deleted;
        report.frame_batches = frames.batches;
        report.blobs_registered = frames.blobs_registered;
        report.history = self
            .history()
            .delete(timeline_id, &UNBOUNDED, &task)
            .await?;
        report.metric_history_deleted = self.delete_metric_history(timeline_id, &task).await?;
        self.delete_external_ids(timeline_id, &config, &mut report, &task)
            .await?;
        self.delete_config_row(timeline_id, &task).await?;
        report.blobs_swept = self.sweep_blobs(options, &task).await?;

        Ok(report)
    }
}

/// `graph_history.delete` takes graph-ID bounds; a timeline wipe wants all of it.
const UNBOUNDED: GraphIDBounds = (None, None);

/// One frame batch's contribution to the tally.
#[derive(Default)]
struct FrameBatch {
    frames_deleted: u64,
    blobs_registered: u64,
}

/// What the whole frame pass got through.
#[derive(Default)]
struct FrameTally {
    frames_deleted: u64,
    batches: u64,
    blobs_registered: u64,
}

// -- Frames -------------------------------------------------------------------

impl Timelines {
    /// Delete every frame, `batch_size` at a time, until none are left.
    ///
    /// The loop terminates on an empty batch rather than on a zero delete
    /// count: the two agree under the timeline lock, but only the former can't
    /// spin if a backend ever disagrees about what it just handed back.
    ///
    /// Counts the timeline first, purely so the run has a denominator — this is
    /// the long pass, and without one there is no way to tell a delete that is
    /// nearly done from one that has barely started. The count can drift under
    /// a concurrent writer; [`progress`] clamps rather than pretending
    /// otherwise.
    #[task]
    async fn delete_all_frames(
        &self,
        timeline_id: &TimelineID,
        options: &TimelineDeleteOptions,
        task: &ll::Task,
    ) -> Result<FrameTally> {
        let total = self.count_frames(timeline_id, &task).await?;
        task.data("frames_to_delete", total);
        task.progress(0, total);

        let mut tally = FrameTally::default();
        let mut rate = Throughput::new();
        let total_batches = chunks(total, options.batch_size);
        loop {
            let label = rate.label(tally.batches as i64 + 1, total_batches, "frames deleted");
            let Some(batch) = task
                .spawn(label, |task| async move {
                    self.delete_frame_batch(timeline_id, options.batch_size, &task)
                        .await
                })
                .await?
            else {
                break;
            };
            tally.frames_deleted += batch.frames_deleted;
            tally.blobs_registered += batch.blobs_registered;
            tally.batches += 1;
            rate.add(batch.frames_deleted);
            task.progress(progress(tally.frames_deleted, total), total);
        }

        // Land the bar on full even if the count was stale.
        task.progress(total, total);
        task.data("frames_deleted", tally.frames_deleted);
        task.data("blobs_registered", tally.blobs_registered);
        Ok(tally)
    }

    async fn count_frames(&self, timeline_id: &TimelineID, task: &ll::Task) -> Result<i64> {
        let mut conn = self.ctx.storage.graph.conn_read().await?;
        conn.count_frames(timeline_id, task).await
    }

    /// Delete the timeline's next `batch_size` frames in one transaction.
    ///
    /// Reads the batch *inside* the transaction, after taking the timeline
    /// lock, and deletes exactly the frames it read — so no frame can slip into
    /// the delete without its blobs having been registered first. Returns
    /// `None` when the timeline has no frames left.
    ///
    /// `with_manifest` rather than `with_data`: the manifest names the blobs,
    /// and the payload beside it would be the frame's whole compressed graph,
    /// times `batch_size`.
    async fn delete_frame_batch(
        &self,
        timeline_id: &TimelineID,
        batch_size: i64,
        task: &ll::Task,
    ) -> Result<Option<FrameBatch>> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;

        let rows = conn
            .select_frames(&batch_query(timeline_id, batch_size), task)
            .await?;
        if rows.is_empty() {
            conn.commit_transaction(task).await?;
            return Ok(None);
        }

        let blob_keys = batch_blob_keys(&rows)?;
        conn.register_blobs_for_cleanup(&blob_keys, task).await?;
        let graph_ids: Vec<GraphID> = rows.iter().map(|row| row.frame.graph_id).collect();
        let frames_deleted = conn.delete_frames(timeline_id, &graph_ids, task).await?;
        conn.commit_transaction(task).await?;

        Ok(Some(FrameBatch {
            frames_deleted,
            blobs_registered: blob_keys.len() as u64,
        }))
    }
}

/// How far along the bar should sit, clamped to the total.
///
/// The total comes from a `COUNT(*)` taken before the first batch, so a writer
/// racing the delete can push the real number of frames past it. Overshooting
/// the denominator would render as a bar past 100%; stalling at the end is the
/// honest failure mode, and the caller squares it up once the loop drains.
fn progress(deleted: u64, total: i64) -> i64 {
    i64::try_from(deleted).unwrap_or(i64::MAX).min(total)
}

/// The next `batch_size` frames of a timeline, manifests but no payloads.
fn batch_query(timeline_id: &TimelineID, batch_size: i64) -> FrameQuery {
    FrameQuery {
        timeline_id: timeline_id.clone(),
        limit: Some(batch_size),
        with_manifest: Some(true),
        with_data: Some(false),
        frame_types: None,
        order: None,
        timestamp_bounds: None,
        graph_id_bounds: None,
        graph_ids: None,
        before: None,
        expires_before: None,
    }
}

/// Every external blob key the batch references.
fn batch_blob_keys(rows: &[FrameRow]) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for row in rows {
        keys.extend(external_blob_keys(row)?);
    }
    Ok(keys)
}

// -- Everything else the timeline owns ----------------------------------------

impl Timelines {
    async fn require_config(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<TimelineConfig> {
        self.get_config(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))
    }

    /// Recorded history lives behind its own namespace handle, which is nothing
    /// but the shared context this one already holds.
    fn history(&self) -> GraphHistory {
        GraphHistory {
            ctx: self.ctx.clone(),
        }
    }

    /// Delete the legacy weekly metric history, one week per statement.
    ///
    /// Rows here are `nodes x weeks` and the table has no `graph_id` to chunk
    /// on, so the week — which is indexed — is the only bound available. The
    /// week list doubles as the progress denominator, for free.
    #[task]
    async fn delete_metric_history(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<u64> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        let weeks = conn.list_metric_history_weeks(timeline_id, &task).await?;

        let total = i64::try_from(weeks.len())?;
        task.data("metric_history_weeks", weeks.len());
        task.progress(0, total);

        let mut deleted = 0;
        for (index, week) in weeks.iter().enumerate() {
            deleted += conn
                .delete_metric_history_for_week(timeline_id, week, &task)
                .await?;
            task.progress(i64::try_from(index)? + 1, total);
        }
        Ok(deleted)
    }

    /// Drop the timeline's external ID mappings, unless another timeline
    /// declares the same namespace.
    ///
    /// A namespace is not owned by the timeline that names it — two timelines
    /// can legitimately share one, and the mappings are the ID allocator's
    /// state, not this timeline's data. Deleting a shared namespace would reset
    /// the survivor's `GraphID` sequence to zero and hand out IDs it has
    /// already used, so a shared one is reported and left alone.
    async fn delete_external_ids(
        &self,
        timeline_id: &TimelineID,
        config: &TimelineConfig,
        report: &mut TimelineDeleteReport,
        task: &ll::Task,
    ) -> Result<()> {
        let Some(namespace) = &config.external_id_namespace else {
            return Ok(());
        };

        if let Some(other) = self
            .other_timeline_using(timeline_id, namespace, task)
            .await?
        {
            report.external_id_namespace_shared_with = Some(other);
            return Ok(());
        }

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        report.external_ids_deleted = conn.delete_external_id_mappings(namespace, task).await?;
        Ok(())
    }

    /// The first other timeline declaring `namespace`, if any.
    async fn other_timeline_using(
        &self,
        timeline_id: &TimelineID,
        namespace: &ExternalIDNamespace,
        task: &ll::Task,
    ) -> Result<Option<TimelineID>> {
        for other in self.list(task).await? {
            if &other == timeline_id {
                continue;
            }
            let config = self.get_config(&other, task).await?;
            if config.and_then(|c| c.external_id_namespace).as_ref() == Some(namespace) {
                return Ok(Some(other));
            }
        }
        Ok(None)
    }

    async fn delete_config_row(&self, timeline_id: &TimelineID, task: &ll::Task) -> Result<()> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.delete_timeline(timeline_id, task).await?;
        Ok(())
    }

    /// Drain the aged cleanup backlog. Best-effort is not enough here — an
    /// operator running this asked for the blobs to go — so a sweep failure
    /// fails the command, and re-running is safe.
    async fn sweep_blobs(&self, options: &TimelineDeleteOptions, task: &ll::Task) -> Result<usize> {
        self.ctx
            .storage
            .sweep_blobs(options.sweep_min_age, None, task)
            .await
    }
}

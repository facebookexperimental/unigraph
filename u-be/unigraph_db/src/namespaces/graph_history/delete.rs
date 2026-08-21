// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Wiping a timeline's history.
//!
//! Unbounded history can reach ~1e9 rows, and a single `DELETE` that large is a
//! real SQLite hazard, so the work is split into graph-ID ranges, each its own
//! transaction. Partial progress is safe and the whole thing is re-runnable:
//! error blobs are only *registered* for cleanup, and the sweeper will not
//! touch them until they age past its `min_age` window.

use anyhow::Result;
use ll::task;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimestampBounds;

use super::GraphHistory;
use super::HistoryDeleteReport;
use crate::namespaces::progress::Throughput;

const DELETE_CHUNK_SIZE: i64 = 10_000;

impl GraphHistory {
    /// Delete all of a timeline's recorded history, including its metric-name
    /// dictionary. The timeline itself and its frames are untouched.
    ///
    /// The chunk range comes from the timeline's frames, which is not quite the
    /// same set as the rows: deleting a frame leaves its history behind, and
    /// such a row can sit outside the span entirely. So the pass ends with one
    /// unbounded delete that sweeps whatever the chunks could not see and drops
    /// the dictionary in the same transaction — without it those rows would
    /// survive a "wipe" with their metric names deleted out from under them,
    /// leaving something that cannot even be decoded. It normally matches
    /// nothing, so the chunking above is still what does the heavy lifting.
    #[task]
    pub(super) async fn delete_all_history(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<HistoryDeleteReport> {
        let frames = self
            .select_history_frames(timeline_id, TimestampBounds::default(), (None, None), &task)
            .await?;
        // No frames left to chunk over — fall back to one unbounded delete.
        let Some(first) = frames.first() else {
            return self
                .delete_bounded(timeline_id, &(None, None), true, &task)
                .await;
        };

        let mut report = HistoryDeleteReport::default();
        let mut start = first.frame.graph_id.0;
        let end = frames
            .last()
            .map_or(first.frame.graph_id.0, |frame| frame.frame.graph_id.0);

        // Chunks span graph-ID space, which is sparse, so this counts ranges
        // swept rather than rows deleted — steady progress, not a row estimate.
        let total = (end - start).div_euclid(DELETE_CHUNK_SIZE) + 1;
        task.data("delete_chunks", total);
        task.progress(0, total);
        let mut done = 0i64;
        let mut rate = Throughput::new();

        while start <= end {
            let upper = end.min(start + DELETE_CHUNK_SIZE - 1);
            let bounds = (Some(GraphID(start)), Some(GraphID(upper)));
            let chunk = task
                .spawn(
                    rate.label(done + 1, total, "rows deleted"),
                    |task| async move {
                        self.delete_bounded(timeline_id, &bounds, false, &task)
                            .await
                    },
                )
                .await?;
            report.entries_deleted += chunk.entries_deleted;
            report.statuses_deleted += chunk.statuses_deleted;
            report.error_blobs_registered += chunk.error_blobs_registered;
            rate.add(chunk.entries_deleted);
            start = upper + 1;
            done += 1;
            task.progress(done, total);
        }
        task.data("rows_deleted", i64::try_from(rate.done)?);

        let rest = self
            .delete_bounded(timeline_id, &(None, None), true, &task)
            .await?;
        report.entries_deleted += rest.entries_deleted;
        report.statuses_deleted += rest.statuses_deleted;
        report.error_blobs_registered += rest.error_blobs_registered;
        report.metrics_deleted = rest.metrics_deleted;
        Ok(report)
    }

    /// Delete history entries and checkpoints within `bounds` in one
    /// transaction, registering any error blobs in the range for cleanup first
    /// so the registration rolls back with the delete if it fails.
    ///
    /// `delete_metrics` also drops the metric-name dictionary. Only pass `true`
    /// when nothing else in the timeline can still reference those ids — a
    /// surviving entry would decode against the wrong names.
    pub(super) async fn delete_bounded(
        &self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
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
        let metrics_deleted = match delete_metrics {
            true => conn.delete_history_metrics(timeline_id, task).await?,
            false => 0,
        };
        conn.commit_transaction(task).await?;

        Ok(HistoryDeleteReport {
            entries_deleted,
            statuses_deleted,
            metrics_deleted,
            error_blobs_registered: error_blob_keys.len(),
        })
    }
}

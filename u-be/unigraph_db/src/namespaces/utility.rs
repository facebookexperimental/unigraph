// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Utility operations — TTL cleanup for expired frames and configs.

use anyhow::Result;
use ll::task;
use unigraph_core::config_key::ConfigKeyLike;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::GraphKey;
use unigraph_timestamp::Timestamp;

use crate::context::UnigraphDbContext;

/// Batch size for cleanup operations — delete at most this many items per call.
const CLEANUP_BATCH_SIZE: i64 = 100;

/// Default minimum age for blob sweep during cleanup.
///
/// Blobs registered for cleanup less than this long ago are skipped, to avoid
/// sweeping blobs from in-flight transactions that haven't committed yet.
const DEFAULT_SWEEP_MIN_AGE: std::time::Duration = std::time::Duration::from_secs(3600);

/// Result of a cleanup operation.
pub struct CleanupResult {
    /// Number of expired frames deleted.
    pub frames_deleted: usize,
    /// Number of expired configs deleted.
    pub configs_deleted: usize,
    /// Number of orphaned blobs swept from external storage.
    pub blobs_swept: usize,
}

/// Handle for utility / maintenance operations.
///
/// Obtained via [`UnigraphDb::utility`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Utility {
    pub(crate) ctx: UnigraphDbContext,
}

impl Utility {
    /// Get frames that have expired (past their `expires_at` timestamp)
    /// within a specific timeline.
    ///
    /// Returns metadata-only `FrameRow`s (no data payload).
    #[task(tags(l3))]
    pub async fn get_expired_frames(
        &self,
        timeline_id: &unigraph_storage_core::TimelineID,
        task: &ll::Task,
    ) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn_read().await?;
        conn.select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                limit: Some(CLEANUP_BATCH_SIZE),
                expires_before: Some(Timestamp::now()),
                ..Default::default()
            },
            &task,
        )
        .await
    }

    /// Get config keys that have expired (past their `expires_at` timestamp).
    #[task(tags(l3))]
    pub async fn get_expired_configs(&self, task: &ll::Task) -> Result<Vec<String>> {
        let mut conn = self.ctx.storage.graph.conn_read().await?;
        conn.select_expired_config_keys(Timestamp::now(), CLEANUP_BATCH_SIZE, &task)
            .await
    }

    /// Delete expired frames and configs, then sweep orphaned blobs.
    ///
    /// Processes up to [`CLEANUP_BATCH_SIZE`] expired frames and configs per call.
    /// Frame deletion uses `delete_frame_on_conn` which registers external blob
    /// keys for cleanup. After all deletions, pending blobs are swept.
    ///
    /// `sweep_min_age` controls the minimum age for blob sweep — blobs registered
    /// less than this long ago are skipped (to avoid sweeping in-flight
    /// transaction blobs). Pass `None` for the default (1 hour).
    #[task(tags(l3))]
    pub async fn cleanup_expired(
        &self,
        sweep_min_age: Option<std::time::Duration>,
        task: &ll::Task,
    ) -> Result<CleanupResult> {
        let frames_deleted = self.cleanup_expired_frames(&task).await?;
        let configs_deleted = self.cleanup_expired_configs(&task).await?;
        let min_age = sweep_min_age.unwrap_or(DEFAULT_SWEEP_MIN_AGE);
        let blobs_swept = self.ctx.storage.sweep_blobs(min_age, &task).await?;

        Ok(CleanupResult {
            frames_deleted,
            configs_deleted,
            blobs_swept,
        })
    }

    /// Generate a globally unique integer ID.
    ///
    /// Acquires a write connection and inserts into the auto-increment
    /// `uniq_ids` table. Safe for concurrent callers.
    #[task]
    pub async fn gen_uniq_id(&self, task: &ll::Task) -> Result<i64> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.gen_uniq_id(&task).await
    }

    async fn cleanup_expired_frames(&self, task: &ll::Task) -> Result<usize> {
        // Iterate all timelines and clean up each one.
        let timelines = {
            let mut conn = self.ctx.storage.graph.conn_read().await?;
            conn.list_timelines(task).await?
        };

        let mut total_deleted = 0;
        for timeline_id in &timelines {
            let expired = self.get_expired_frames(timeline_id, task).await?;

            // One transaction per frame — frames are heavy to delete
            // (manifest read + blob registration) and we don't want to
            // hold the transaction open for the entire batch.
            for row in &expired {
                let key = GraphKey {
                    timeline_id: row.timeline_id.clone(),
                    graph_id: row.frame.graph_id,
                };
                let mut conn = self.ctx.storage.graph.conn_write().await?;
                conn.start_transaction(task).await?;
                let was_deleted = self
                    .ctx
                    .storage
                    .delete_frame_on_conn(&mut *conn, &key, task)
                    .await?;
                conn.commit_transaction(task).await?;
                if was_deleted {
                    total_deleted += 1;
                }
            }
        }

        Ok(total_deleted)
    }

    async fn cleanup_expired_configs(&self, task: &ll::Task) -> Result<usize> {
        let expired_keys = self.get_expired_configs(task).await?;
        if expired_keys.is_empty() {
            return Ok(0);
        }

        let mut conn = self.ctx.storage.graph.conn_write().await?;

        // For each expired config, register its blob_id for cleanup (if it
        // has one offloaded to external storage), then delete the row.
        let mut deleted = 0;
        for key in &expired_keys {
            // Read the config row to check for an external blob_id.
            let blob_id = if key.starts_with(TraversalConfigKey::PREFIX) {
                let config_key: TraversalConfigKey = key.parse()?;
                conn.get_traversal_config(&config_key, task)
                    .await?
                    .and_then(|r| r.blob_id)
            } else if key.starts_with(GraphQueryConfigKey::PREFIX) {
                let config_key: GraphQueryConfigKey = key.parse()?;
                conn.get_graph_query_config(&config_key, task)
                    .await?
                    .and_then(|r| r.blob_id)
            } else {
                None
            };

            // Register the external blob for cleanup if present.
            if let Some(ref bid) = blob_id {
                conn.register_blobs_for_cleanup(&[bid.clone()], task)
                    .await?;
            }

            let was_deleted = conn.delete_config_db_rows(key, task).await?;
            if was_deleted {
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

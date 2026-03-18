// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Frame queries — select, list, and inspect frames in a timeline.

use anyhow::Result;
use ll::task;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::context::UnigraphDbContext;

/// Handle for frame operations.
///
/// Obtained via [`UnigraphDb::frames`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Frames {
    pub(crate) ctx: UnigraphDbContext,
}

impl Frames {
    /// Store an empty frame (placeholder with no data).
    ///
    /// Transactional: locks the timeline, validates ordering (AdjacentDeltas
    /// only), stores the frame, and commits.
    #[task(tags(l3))]
    pub async fn store_empty(&self, key: &GraphTimeKey, task: &ll::Task) -> Result<()> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(&task).await?;
        let config = conn
            .get_timeline_config_and_lock(&key.timeline_id, &task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", key.timeline_id))?;

        if matches!(
            config.schema,
            unigraph_storage_core::TimelineSchema::AdjacentDeltas(_)
        ) {
            crate::schemas::adjacent_deltas::validate_monotonic_append(&mut *conn, key, &task)
                .await?;
        }

        conn.store_frame_empty(key, &task).await?;
        conn.commit_transaction(&task).await?;
        Ok(())
    }

    /// Select frames matching a structured query.
    #[task(tags(l3))]
    pub async fn select(&self, query: &FrameQuery, task: &ll::Task) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.select_frames(query, &task).await
    }

    /// Fetch a single frame by graph key.
    ///
    /// - `with_data: false` → fast metadata-only read
    /// - `with_data: true` → includes manifest + blobs
    ///
    /// Returns `None` if the frame does not exist.
    #[task(tags(l3))]
    pub async fn get(
        &self,
        key: &GraphKey,
        with_data: bool,
        task: &ll::Task,
    ) -> Result<Option<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: key.timeline_id.clone(),
                    limit: Some(1),
                    frame_types: None,
                    order: None,
                    timestamp_bounds: None,
                    graph_id_bounds: None,
                    graph_ids: Some(vec![key.graph_id]),
                    with_data: Some(with_data),
                    before: None,
                },
                &task,
            )
            .await?;
        Ok(rows.pop())
    }

    /// List all frames in a timeline, ordered by (timestamp, graph_id).
    /// Returns metadata only (data is `None`).
    #[task(tags(l3))]
    pub async fn list(&self, timeline_id: &TimelineID, task: &ll::Task) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                limit: None,
                frame_types: None,
                order: None,
                timestamp_bounds: None,
                graph_id_bounds: None,
                graph_ids: None,
                with_data: None,
                before: None,
            },
            &task,
        )
        .await
    }

    /// List frames in a timeline within a time range.
    /// Returns metadata only (data is `None`).
    #[task(tags(l3))]
    pub async fn list_range(
        &self,
        timeline_id: &TimelineID,
        start: Timestamp,
        end: Timestamp,
        task: &ll::Task,
    ) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                limit: None,
                frame_types: None,
                order: None,
                timestamp_bounds: Some(TimestampBounds {
                    start: Some(start),
                    end: Some(end),
                }),
                graph_id_bounds: None,
                graph_ids: None,
                with_data: None,
                before: None,
            },
            &task,
        )
        .await
    }

    /// Get the frame immediately preceding the given key in the timeline.
    /// Returns metadata only (data is `None`).
    #[task(tags(l3))]
    pub async fn get_preceding(
        &self,
        key: &GraphTimeKey,
        task: &ll::Task,
    ) -> Result<Option<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: key.timeline_id.clone(),
                    limit: None,
                    frame_types: None,
                    order: None,
                    timestamp_bounds: None,
                    graph_id_bounds: None,
                    graph_ids: None,
                    with_data: None,
                    before: Some((key.timestamp, key.graph_id)),
                },
                &task,
            )
            .await?;
        Ok(rows.pop())
    }
}

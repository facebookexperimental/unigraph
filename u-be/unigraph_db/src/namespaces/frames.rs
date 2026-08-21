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
    /// Select frames matching a structured query.
    #[task(tags(l3))]
    pub async fn select(&self, query: &FrameQuery, task: &ll::Task) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.select_frames(query, &task).await
    }

    /// Select frames using the analytics connection (dedicated pool / longer timeouts).
    #[task(tags(l3))]
    pub async fn select_analytics(
        &self,
        query: &FrameQuery,
        task: &ll::Task,
    ) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn_analytics().await?;
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
                    with_manifest: None,
                    with_data: Some(with_data),
                    before: None,
                    expires_before: None,
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
                with_manifest: None,
                with_data: None,
                before: None,
                expires_before: None,
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
                with_manifest: None,
                with_data: None,
                before: None,
                expires_before: None,
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
                    with_manifest: None,
                    with_data: None,
                    before: Some((key.timestamp, key.graph_id)),
                    expires_before: None,
                },
                &task,
            )
            .await?;
        Ok(rows.pop())
    }
}

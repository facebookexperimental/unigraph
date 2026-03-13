// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Frame queries — select, list, and inspect frames in a timeline.

use std::sync::Arc;

use anyhow::Result;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::storage::UnigraphStorage;

/// Handle for frame operations.
///
/// Obtained via [`UnigraphDb::frames`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Frames {
    pub(crate) storage: Arc<UnigraphStorage>,
}

impl Frames {
    /// Store an empty frame (placeholder with no data).
    ///
    /// Transactional: locks the timeline, validates ordering (AdjacentDeltas
    /// only), stores the frame, and commits.
    pub async fn store_empty(&self, key: &GraphTimeKey) -> Result<()> {
        let mut conn = self.storage.graph.conn().await?;
        conn.start_transaction().await?;
        let config = conn
            .get_timeline_config_and_lock(&key.timeline_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", key.timeline_id))?;

        if matches!(
            config.schema,
            unigraph_storage_core::TimelineSchema::AdjacentDeltas(_)
        ) {
            crate::schemas::adjacent_deltas::validate_monotonic_append(&mut *conn, key).await?;
        }

        conn.store_frame_empty(key).await?;
        conn.commit_transaction().await?;
        Ok(())
    }

    /// Select frames matching a structured query.
    pub async fn select(&self, query: &FrameQuery) -> Result<Vec<FrameRow>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.select_frames(query).await
    }

    /// Fetch a single frame by graph key.
    ///
    /// - `with_data: false` → fast metadata-only read
    /// - `with_data: true` → includes manifest + blobs
    ///
    /// Returns `None` if the frame does not exist.
    pub async fn get(&self, key: &GraphKey, with_data: bool) -> Result<Option<FrameRow>> {
        let mut conn = self.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(&FrameQuery {
                timeline_id: key.timeline_id.clone(),
                limit: Some(1),
                frame_types: None,
                order: None,
                timestamp_bounds: None,
                graph_id_bounds: None,
                graph_ids: Some(vec![key.graph_id]),
                with_data: Some(with_data),
                before: None,
            })
            .await?;
        Ok(rows.pop())
    }

    /// List all frames in a timeline, ordered by (timestamp, graph_id).
    /// Returns metadata only (data is `None`).
    pub async fn list(&self, timeline_id: &TimelineID) -> Result<Vec<FrameRow>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.select_frames(&FrameQuery {
            timeline_id: timeline_id.clone(),
            limit: None,
            frame_types: None,
            order: None,
            timestamp_bounds: None,
            graph_id_bounds: None,
            graph_ids: None,
            with_data: None,
            before: None,
        })
        .await
    }

    /// List frames in a timeline within a time range.
    /// Returns metadata only (data is `None`).
    pub async fn list_range(
        &self,
        timeline_id: &TimelineID,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Vec<FrameRow>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.select_frames(&FrameQuery {
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
        })
        .await
    }

    /// Get the frame immediately preceding the given key in the timeline.
    /// Returns metadata only (data is `None`).
    pub async fn get_preceding(&self, key: &GraphTimeKey) -> Result<Option<FrameRow>> {
        let mut conn = self.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(&FrameQuery {
                timeline_id: key.timeline_id.clone(),
                limit: None,
                frame_types: None,
                order: None,
                timestamp_bounds: None,
                graph_id_bounds: None,
                graph_ids: None,
                with_data: None,
                before: Some((key.timestamp, key.graph_id)),
            })
            .await?;
        Ok(rows.pop())
    }
}

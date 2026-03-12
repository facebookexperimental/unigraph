// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Graph domain operations — store, fetch, compact, and delete graphs.
//!
//! Fetch, compact, and delete dispatch to the appropriate schema implementation
//! based on the timeline's [`TimelineSchema`]. Store operations are schema-agnostic
//! and delegate to [`UnigraphStorage`].

use std::sync::Arc;

use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampedError;

use crate::schemas::adjacent_deltas;
use crate::storage::UnigraphStorage;

/// Handle for graph domain operations.
///
/// Obtained via [`UnigraphDb::graph`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Graph {
    pub(crate) storage: Arc<UnigraphStorage>,
}

// -- Public API --

impl Graph {
    /// Store a full graph snapshot.
    pub async fn store_full(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        self.storage.store_graph_full(key, graph).await
    }

    /// Store a delta-compressed graph.
    pub async fn store_delta(
        &self,
        key: &GraphTimeKey,
        base_key: &GraphKey,
        target_graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        self.storage
            .store_graph_delta(key, base_key, target_graph)
            .await
    }

    /// Store error data for a failed graph computation.
    pub async fn store_error(&self, key: &GraphTimeKey, errors: &[TimestampedError]) -> Result<()> {
        self.storage.store_error(key, errors).await
    }

    /// Fetch and reconstruct a graph from storage.
    ///
    /// Dispatches to the schema-specific fetch implementation based on the
    /// timeline's configuration.
    pub async fn fetch(&self, key: &GraphKey) -> Result<ArrayGraphSerializable> {
        let schema = self.get_timeline_schema(&key.timeline_id).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                adjacent_deltas::fetch_graph(&self.storage, key).await
            }
        }
    }

    /// Fetch the latest reconstructable graph from a timeline.
    ///
    /// Finds the most recent `Full` or `Delta` frame (skipping `Empty` and `Error`)
    /// and reconstructs the graph from it.
    pub async fn fetch_latest(
        &self,
        timeline_id: &TimelineID,
    ) -> Result<(GraphKey, ArrayGraphSerializable)> {
        let frame = self.find_latest_fetchable_frame(timeline_id).await?;
        let key = GraphKey {
            timeline_id: timeline_id.clone(),
            graph_id: frame.frame.graph_id,
        };
        let graph = self.fetch(&key).await?;
        Ok((key, graph))
    }

    /// Fetch errors for a frame.
    pub async fn fetch_errors(&self, key: &GraphKey) -> Result<Vec<TimestampedError>> {
        self.storage.fetch_errors(key).await
    }

    /// Compact a timeline by replacing consecutive Full frames with Deltas.
    ///
    /// Dispatches to the schema-specific compaction implementation.
    /// Returns the number of frames converted from Full to Delta.
    pub async fn compact(
        &self,
        timeline_id: &TimelineID,
        start: Option<Timestamp>,
        end: Option<Timestamp>,
    ) -> Result<usize> {
        let schema = self.get_timeline_schema(timeline_id).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                adjacent_deltas::compact_timeline(&self.storage, timeline_id, start, end).await
            }
        }
    }

    /// Delete a frame and register its external blobs for cleanup.
    ///
    /// Dispatches to the schema-specific delete implementation.
    pub async fn delete(&self, key: &GraphKey, timeline_id: &TimelineID) -> Result<bool> {
        let schema = self.get_timeline_schema(timeline_id).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                self.delete_adjacent_deltas(key, timeline_id).await
            }
        }
    }
}

// -- Private helpers --

impl Graph {
    async fn get_timeline_schema(&self, timeline_id: &TimelineID) -> Result<TimelineSchema> {
        let mut conn = self.storage.graph.conn().await?;
        let config = conn
            .get_timeline_config(timeline_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
        Ok(config.schema)
    }

    async fn find_latest_fetchable_frame(
        &self,
        timeline_id: &TimelineID,
    ) -> Result<unigraph_storage_core::FrameRow> {
        let mut conn = self.storage.graph.conn().await?;
        let mut frames = conn
            .select_frames(&FrameQuery {
                timeline_id: timeline_id.clone(),
                limit: Some(1),
                frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                order: Some(Order::Desc),
                timestamp_bounds: None,
                graph_id_bounds: None,
                graph_ids: None,
                with_data: Some(false),
                before: None,
            })
            .await?;

        frames.pop().ok_or_else(|| {
            anyhow::anyhow!("No fetchable graph found in timeline '{}'", timeline_id)
        })
    }

    async fn delete_adjacent_deltas(
        &self,
        key: &GraphKey,
        timeline_id: &TimelineID,
    ) -> Result<bool> {
        let mut conn = self.storage.graph.conn().await?;
        conn.start_transaction().await?;
        conn.get_timeline_config_and_lock(timeline_id).await?;
        let deleted = self.storage.delete_frame_on_conn(&mut *conn, key).await?;
        conn.commit_transaction().await?;
        Ok(deleted)
    }
}

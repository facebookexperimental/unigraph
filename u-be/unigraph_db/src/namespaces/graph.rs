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
use crate::schemas::full_or_delta;
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
    /// Store a graph snapshot.
    ///
    /// Dispatches to the schema-specific store implementation:
    /// - AdjacentDeltas: validates monotonic ordering, stores as Full
    /// - FullOrDelta: stores as Full with no ordering validation
    pub async fn store(&self, key: &GraphTimeKey, graph: &ArrayGraphSerializable) -> Result<()> {
        let schema = self.get_timeline_schema(&key.timeline_id).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                adjacent_deltas::store_full(&self.storage, key, graph).await
            }
            TimelineSchema::FullOrDelta(_) => {
                full_or_delta::store_full(&self.storage, key, graph).await
            }
        }
    }

    /// Explicitly store a graph as a delta from another graph.
    ///
    /// Fetches the base graph, derives the delta internally, and stores it.
    /// Cross-timeline bases are allowed for FullOrDelta timelines.
    ///
    /// Only supported for FullOrDelta timelines. AdjacentDeltas manages
    /// deltas via compaction — explicit delta storage is not allowed.
    pub async fn store_as_delta_from(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
        from_key: &GraphKey,
    ) -> Result<()> {
        let schema = self.get_timeline_schema(&key.timeline_id).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                anyhow::bail!(
                    "store_as_delta_from is not supported for AdjacentDeltas timelines \
                     (deltas are managed via compaction)"
                )
            }
            TimelineSchema::FullOrDelta(_) => {
                full_or_delta::store_delta(&self.storage, key, from_key, graph).await
            }
        }
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
        self.storage.fetch_graph(key).await
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
            TimelineSchema::FullOrDelta(_) => {
                full_or_delta::compact_timeline(&self.storage, timeline_id, start, end).await
            }
        }
    }

    /// Delete a frame and register its external blobs for cleanup.
    ///
    /// Dispatches to the schema-specific delete implementation.
    pub async fn delete(&self, key: &GraphKey, timeline_id: &TimelineID) -> Result<bool> {
        let schema = self.get_timeline_schema(timeline_id).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) | TimelineSchema::FullOrDelta(_) => {
                self.delete_with_lock(key, timeline_id).await
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

    async fn delete_with_lock(&self, key: &GraphKey, timeline_id: &TimelineID) -> Result<bool> {
        let mut conn = self.storage.graph.conn().await?;
        conn.start_transaction().await?;
        conn.get_timeline_config_and_lock(timeline_id).await?;
        let deleted = self.storage.delete_frame_on_conn(&mut *conn, key).await?;
        conn.commit_transaction().await?;
        Ok(deleted)
    }
}

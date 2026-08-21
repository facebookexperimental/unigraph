// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Graph domain operations — store, fetch, compact, and delete graphs.
//!
//! Fetch, compact, and delete dispatch to the appropriate schema implementation
//! based on the timeline's [`TimelineSchema`]. Store operations are schema-agnostic
//! and delegate to [`UnigraphStorage`](crate::storage::UnigraphStorage).

use anyhow::Result;
use ll::task;
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

use super::AdjacentDeltasOps;
use crate::context::UnigraphDbContext;
use crate::schemas::adjacent_deltas;
use crate::schemas::full_or_delta;

/// On every frame delete, opportunistically sweep external blobs that were
/// registered for cleanup at least this long ago. The window is deliberately
/// generous (well past any commit latency) so the piggybacked sweep never
/// touches blobs from in-flight or not-yet-committed transactions.
const SWEEP_ON_DELETE_MIN_AGE: std::time::Duration = std::time::Duration::from_secs(2 * 60 * 60);

/// Cap for the piggybacked per-delete sweep, so a large cleanup backlog can't
/// turn a single delete into an unbounded amount of external-storage work.
const SWEEP_ON_DELETE_LIMIT: i64 = 200;

/// Handle for graph domain operations.
///
/// Obtained via [`UnigraphDb::graph`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Graph {
    pub(crate) ctx: UnigraphDbContext,
    /// Batch operations for adjacent deltas timelines (store/load ranges).
    pub adjacent_deltas: AdjacentDeltasOps,
}

// -- Public API --

impl Graph {
    /// Store a graph snapshot.
    ///
    /// Dispatches to the schema-specific store implementation:
    /// - AdjacentDeltas: validates monotonic ordering, stores as Full
    /// - FullOrDelta: stores as Full with no ordering validation
    #[task(tags(l3))]
    pub async fn store(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()> {
        let schema = self.get_timeline_schema(&key.timeline_id, &task).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                adjacent_deltas::store_full(&self.ctx, key, graph, expires_at, &task).await
            }
            TimelineSchema::FullOrDelta(_) => {
                full_or_delta::store_full(&self.ctx, key, graph, expires_at, &task).await
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
    #[task(tags(l3))]
    pub async fn store_as_delta_from(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
        from_key: &GraphKey,
        task: &ll::Task,
    ) -> Result<()> {
        let schema = self.get_timeline_schema(&key.timeline_id, &task).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                anyhow::bail!(
                    "store_as_delta_from is not supported for AdjacentDeltas timelines \
                     (deltas are managed via compaction)"
                )
            }
            TimelineSchema::FullOrDelta(_) => {
                full_or_delta::store_delta(&self.ctx, key, from_key, graph, &task).await
            }
        }
    }

    /// Store error data for a failed graph computation.
    #[task(tags(l3))]
    pub async fn store_error(
        &self,
        key: &GraphTimeKey,
        errors: &[TimestampedError],
        task: &ll::Task,
    ) -> Result<()> {
        let config = self.ctx.pack_config_for_key(key);
        self.ctx
            .storage
            .store_error(key, errors, &config, &task)
            .await
    }

    /// Fetch and reconstruct a graph from storage.
    ///
    /// Dispatches to the schema-specific fetch implementation based on the
    /// timeline's configuration.
    #[task(tags(l3))]
    pub async fn fetch(&self, key: &GraphKey, task: &ll::Task) -> Result<ArrayGraphSerializable> {
        self.ctx.storage.fetch_graph(key, &task).await
    }

    /// Fetch the latest reconstructable graph from a timeline.
    ///
    /// Finds the most recent `Full` or `Delta` frame (skipping `Empty` and `Error`)
    /// and reconstructs the graph from it.
    #[task]
    pub async fn fetch_latest(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<(GraphKey, ArrayGraphSerializable)> {
        let frame = self.find_latest_fetchable_frame(timeline_id, &task).await?;
        let key = GraphKey {
            timeline_id: timeline_id.clone(),
            graph_id: frame.frame.graph_id,
        };
        let graph = self.ctx.storage.fetch_graph(&key, &task).await?;
        Ok((key, graph))
    }

    /// Fetch errors for a frame.
    #[task(tags(l3))]
    pub async fn fetch_errors(
        &self,
        key: &GraphKey,
        task: &ll::Task,
    ) -> Result<Vec<TimestampedError>> {
        self.ctx.storage.fetch_errors(key, &task).await
    }

    /// Compact a timeline by replacing consecutive Full frames with Deltas.
    ///
    /// Dispatches to the schema-specific compaction implementation.
    /// Returns the number of frames converted from Full to Delta.
    #[task(tags(l3))]
    pub async fn compact(
        &self,
        timeline_id: &TimelineID,
        start: Option<Timestamp>,
        end: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<usize> {
        let schema = self.get_timeline_schema(timeline_id, &task).await?;
        match schema {
            TimelineSchema::AdjacentDeltas(_) => {
                adjacent_deltas::compact_timeline(&self.ctx, timeline_id, start, end, &task).await
            }
            TimelineSchema::FullOrDelta(_) => {
                full_or_delta::compact_timeline(&self.ctx.storage, timeline_id, start, end).await
            }
        }
    }

    /// Delete a frame and register its external blobs for cleanup.
    ///
    /// Dispatches to the schema-specific delete implementation, then
    /// opportunistically sweeps a bounded batch of aged, pending-deletion blobs
    /// (see [`sweep_pending_blobs_best_effort`]) so a separate sweeper job isn't
    /// required. The sweep runs regardless of whether this frame existed or had
    /// external blobs — most deletes register nothing, but the cleanup backlog
    /// is shared, so any delete is a fine moment to drain a little of it.
    #[task(tags(l3))]
    pub async fn delete(
        &self,
        key: &GraphKey,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<bool> {
        let schema = self.get_timeline_schema(timeline_id, &task).await?;
        let deleted = match schema {
            TimelineSchema::AdjacentDeltas(_) | TimelineSchema::FullOrDelta(_) => {
                self.delete_with_lock(key, timeline_id, &task).await?
            }
        };
        self.sweep_pending_blobs_best_effort(&task).await;
        Ok(deleted)
    }
}

// -- Private helpers --

impl Graph {
    async fn get_timeline_schema(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<TimelineSchema> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let config = conn
            .get_timeline_config(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
        Ok(config.schema)
    }

    async fn find_latest_fetchable_frame(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<unigraph_storage_core::FrameRow> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let mut frames = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: timeline_id.clone(),
                    limit: Some(1),
                    frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                    order: Some(Order::Desc),
                    timestamp_bounds: None,
                    graph_id_bounds: None,
                    graph_ids: None,
                    with_manifest: None,
                    with_data: Some(false),
                    before: None,
                    expires_before: None,
                },
                task,
            )
            .await?;

        frames.pop().ok_or_else(|| {
            anyhow::anyhow!("No fetchable graph found in timeline '{}'", timeline_id)
        })
    }

    /// Opportunistically sweep a bounded batch of aged, pending-deletion blobs.
    ///
    /// Runs after every delete as a piggybacked GC pass, so `delete` callers
    /// don't need a separate sweeper job. Best-effort by design: the blobs it
    /// targets were registered long ago (not by this delete — those are well
    /// under [`SWEEP_ON_DELETE_MIN_AGE`]), and a sweep failure is logged on the
    /// task and swallowed so it can never fail the delete it rode in on.
    ///
    /// The underlying sweep drains newest-first: `sweep_blobs` deletes its whole
    /// batch atomically, so a single blob that fails to delete fails the batch.
    /// Newest-first keeps a persistently-failing *old* blob from blocking the
    /// cleanup of freshly-registered ones under the [`SWEEP_ON_DELETE_LIMIT`]
    /// cap (which would otherwise let new blobs pile up unbounded).
    async fn sweep_pending_blobs_best_effort(&self, task: &ll::Task) {
        match self
            .ctx
            .storage
            .sweep_blobs(SWEEP_ON_DELETE_MIN_AGE, Some(SWEEP_ON_DELETE_LIMIT), task)
            .await
        {
            Ok(swept) => task.data("blobs_swept_on_delete", swept),
            Err(e) => task.data("blobs_swept_on_delete_error", format!("{e:#}")),
        }
    }

    async fn delete_with_lock(
        &self,
        key: &GraphKey,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<bool> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task).await?;
        let deleted = self
            .ctx
            .storage
            .delete_frame_on_conn(&mut *conn, key, task)
            .await?;
        conn.commit_transaction(task).await?;
        Ok(deleted)
    }
}

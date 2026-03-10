// Copyright (c) Meta Platforms, Inc. and affiliates.

//! High-level graph database for Unigraph.
//!
//! [`UnigraphDb`] is the single entry point for all storage operations.
//! It wraps an [`UnigraphStorage`] compositor internally (which combines a
//! [`UnigraphGraphStorage`](unigraph_storage_core::UnigraphGraphStorage) backend
//! with a [`UnigraphBlobStorage`](unigraph_storage_core::UnigraphBlobStorage)
//! backend) and provides a unified API for graph lifecycle management.
//!
//! `UnigraphDb` is `Clone` (via `Arc`) and can be passed freely across threads.

mod adjacent_deltas;
mod frame_storage;
mod storage;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
pub use storage::UnigraphStorage;
use unigraph_core::ArrayGraphSerializable;
use unigraph_storage_core::ExternalID;
use unigraph_storage_core::ExternalIDNamespace;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;
use unigraph_storage_core::TimestampedError;
use unigraph_storage_core::UnigraphBlobStorage;
use unigraph_storage_core::UnigraphGraphConnection;
use unigraph_storage_core::UnigraphGraphStorage;

/// High-level graph database — the single entry point for all storage operations.
///
/// Wraps an [`UnigraphStorage`] compositor and exposes a unified API.
/// `Clone` via internal `Arc`, safe to share across threads.
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
///
/// use unigraph_db::UnigraphDb;
///
/// // Assuming you have graph and blob storage implementations:
/// // let graph: Arc<dyn UnigraphGraphStorage> = ...;
/// // let blob: Arc<dyn UnigraphBlobStorage> = ...;
/// // let db = UnigraphDb::new(graph, blob);
/// // let db2 = db.clone(); // cheap clone via Arc
/// ```
#[derive(Clone)]
pub struct UnigraphDb {
    storage: Arc<UnigraphStorage>,
}

impl UnigraphDb {
    /// Create a new `UnigraphDb` from graph and blob storage backends.
    pub fn new(graph: Arc<dyn UnigraphGraphStorage>, blob: Arc<dyn UnigraphBlobStorage>) -> Self {
        Self {
            storage: Arc::new(UnigraphStorage::new(graph, blob)),
        }
    }

    // -- Timeline operations (delegate through a connection) --

    /// Create a new timeline with the given configuration.
    pub async fn create_timeline(
        &self,
        timeline_id: &TimelineID,
        config: &TimelineConfig,
    ) -> Result<()> {
        let conn = self.storage.graph.conn().await?;
        conn.create_timeline(timeline_id, config).await
    }

    /// Get the configuration for an existing timeline.
    /// Returns `None` if the timeline does not exist.
    pub async fn get_timeline_config(
        &self,
        timeline_id: &TimelineID,
    ) -> Result<Option<TimelineConfig>> {
        let conn = self.storage.graph.conn().await?;
        conn.get_timeline_config(timeline_id).await
    }

    /// List all timeline IDs.
    pub async fn list_timelines(&self) -> Result<Vec<TimelineID>> {
        let conn = self.storage.graph.conn().await?;
        conn.list_timelines().await
    }

    // -- Frame operations --

    /// Store an empty frame (placeholder with no data).
    ///
    /// Transactional: locks the timeline, validates monotonic ordering,
    /// stores the frame, and commits.
    pub async fn store_frame_empty(&self, key: &GraphTimeKey) -> Result<()> {
        let conn = self.storage.graph.conn().await?;
        conn.start_transaction().await?;
        conn.get_timeline_config_and_lock(&key.timeline_id).await?;

        crate::adjacent_deltas::validate_monotonic_append(&*conn, key).await?;

        conn.store_frame_empty(key).await?;
        conn.commit_transaction().await?;
        Ok(())
    }

    /// Select frames matching a structured query.
    pub async fn select_frames(&self, query: &FrameQuery) -> Result<Vec<FrameRow>> {
        let conn = self.storage.graph.conn().await?;
        conn.select_frames(query).await
    }

    /// Fetch a single frame by graph key.
    ///
    /// - `with_data: false` → fast metadata-only read
    /// - `with_data: true` → includes manifest + blobs
    ///
    /// Returns `None` if the frame does not exist.
    pub async fn get_frame(&self, key: &GraphKey, with_data: bool) -> Result<Option<FrameRow>> {
        let conn = self.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(&FrameQuery {
                timeline_id: key.timeline_id.clone(),
                graph_ids: Some(vec![key.graph_id]),
                with_data: Some(with_data),
                limit: Some(1),
                ..Default::default()
            })
            .await?;
        Ok(rows.pop())
    }

    /// List all frames in a timeline, ordered by (timestamp, graph_id).
    /// Returns metadata only (data is `None`).
    pub async fn list_frames(&self, timeline_id: &TimelineID) -> Result<Vec<FrameRow>> {
        let conn = self.storage.graph.conn().await?;
        conn.select_frames(&FrameQuery {
            timeline_id: timeline_id.clone(),
            ..Default::default()
        })
        .await
    }

    /// List frames in a timeline within a time range.
    /// Returns metadata only (data is `None`).
    pub async fn list_frames_range(
        &self,
        timeline_id: &TimelineID,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Vec<FrameRow>> {
        let conn = self.storage.graph.conn().await?;
        conn.select_frames(&FrameQuery {
            timeline_id: timeline_id.clone(),
            timestamp_bounds: Some(TimestampBounds {
                start: Some(start),
                end: Some(end),
            }),
            ..Default::default()
        })
        .await
    }

    /// Get the frame immediately preceding the given key in the timeline.
    /// Returns metadata only (data is `None`).
    pub async fn get_preceding_frame(&self, key: &GraphTimeKey) -> Result<Option<FrameRow>> {
        let conn = self.storage.graph.conn().await?;
        let mut rows = conn
            .select_frames(&FrameQuery {
                timeline_id: key.timeline_id.clone(),
                before: Some((key.timestamp, key.graph_id)),
                ..Default::default()
            })
            .await?;
        Ok(rows.pop())
    }

    /// Get all blob keys that are pending cleanup.
    pub async fn get_blobs_pending_cleanup(&self) -> Result<Vec<String>> {
        let conn = self.storage.graph.conn().await?;
        conn.get_blobs_pending_cleanup().await
    }

    // -- Raw connection access --

    /// Get a raw graph connection for manual transaction control.
    ///
    /// Most callers should use the high-level `UnigraphDb` methods instead.
    /// Use this only when you need to hold a transaction across multiple operations
    /// (e.g. the ingestion pipeline's registration phase).
    pub async fn graph_conn(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.storage.graph.conn().await
    }

    // -- External ID mapping operations --

    /// Register new ExternalIDs and allocate sequential GraphIDs.
    ///
    /// Manages lock + transaction internally:
    /// 1. Acquires a named lock for the namespace
    /// 2. Starts an exclusive transaction
    /// 3. Loads existing mappings (may have moved since caller checked)
    /// 4. Validates linear history (overlapping prefix must be contiguous)
    /// 5. Allocates sequential GraphIDs for the new tail
    /// 6. Commits the transaction and releases the lock
    ///
    /// Returns GraphIDs for ALL input ExternalIDs (both pre-existing and
    /// newly allocated), in the same order as the input.
    pub async fn add_new_external_ids(
        &self,
        ns: &ExternalIDNamespace,
        external_ids: &[ExternalID],
    ) -> Result<Vec<GraphID>> {
        if external_ids.is_empty() {
            return Ok(vec![]);
        }

        let lock_name = format!("external_ids:{}", ns.0);
        let conn = self.storage.graph.conn().await?;
        conn.acquire_named_lock(&lock_name).await?;
        conn.start_transaction().await?;

        let result = resolve_and_allocate(&*conn, ns, external_ids).await;

        finish_transaction(&*conn, result, &lock_name).await
    }

    /// Look up the ExternalID for a GraphID within a namespace.
    pub async fn graph_id_to_external_id(
        &self,
        external_id_namespace: &ExternalIDNamespace,
        graph_id: &GraphID,
    ) -> Result<Option<ExternalID>> {
        let conn = self.storage.graph.conn().await?;
        conn.graph_id_to_external_id(external_id_namespace, graph_id)
            .await
    }

    /// Look up ExternalIDs for multiple GraphIDs within a namespace (batch).
    pub async fn graph_ids_to_external_ids(
        &self,
        external_id_namespace: &ExternalIDNamespace,
        graph_ids: &[GraphID],
    ) -> Result<Vec<(GraphID, ExternalID)>> {
        let conn = self.storage.graph.conn().await?;
        conn.graph_ids_to_external_ids(external_id_namespace, graph_ids)
            .await
    }

    /// Get the ExternalID with the highest GraphID in a namespace.
    pub async fn get_latest_external_id(
        &self,
        external_id_namespace: &ExternalIDNamespace,
    ) -> Result<Option<ExternalID>> {
        let conn = self.storage.graph.conn().await?;
        conn.get_latest_external_id(external_id_namespace).await
    }

    // -- Graph domain operations (compositor) --

    /// Store a full graph snapshot.
    pub async fn store_graph_full(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        self.storage.store_graph_full(key, graph).await
    }

    /// Store a delta-compressed graph.
    pub async fn store_graph_delta(
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
    /// Uses iterative delta chain resolution: finds the nearest Full frame,
    /// loads the range, and folds deltas forward. See [`adjacent_deltas`].
    pub async fn fetch_graph(&self, key: &GraphKey) -> Result<ArrayGraphSerializable> {
        self.storage.fetch_graph(key).await
    }

    /// Fetch the latest reconstructable graph from a timeline.
    ///
    /// Finds the most recent `Full` or `Delta` frame (skipping `Empty` and `Error`)
    /// and reconstructs the graph from it.
    pub async fn fetch_latest_graph(
        &self,
        timeline_id: &TimelineID,
    ) -> Result<(GraphKey, ArrayGraphSerializable)> {
        let frames = self
            .select_frames(&FrameQuery {
                timeline_id: timeline_id.clone(),
                frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                order: Some(Order::Desc),
                limit: Some(1),
                with_data: Some(false),
                ..Default::default()
            })
            .await?;

        let frame = frames.into_iter().next().ok_or_else(|| {
            anyhow::anyhow!("No fetchable graph found in timeline '{}'", timeline_id)
        })?;

        let key = GraphKey {
            timeline_id: timeline_id.clone(),
            graph_id: frame.frame.graph_id,
        };
        let graph = self.fetch_graph(&key).await?;
        Ok((key, graph))
    }

    /// Fetch errors for a frame.
    pub async fn fetch_errors(&self, key: &GraphKey) -> Result<Vec<TimestampedError>> {
        self.storage.fetch_errors(key).await
    }

    /// Delete a frame and register its external blobs for cleanup.
    ///
    /// Starts a transaction, locks the timeline, deletes the frame,
    /// registers external blobs for cleanup, and commits.
    pub async fn delete_frame(&self, key: &GraphKey, timeline_id: &TimelineID) -> Result<bool> {
        let conn = self.storage.graph.conn().await?;
        conn.start_transaction().await?;
        conn.get_timeline_config_and_lock(timeline_id).await?;
        let deleted = self.storage.delete_frame_on_conn(&*conn, key).await?;
        conn.commit_transaction().await?;
        Ok(deleted)
    }

    /// Sweep external blobs that have been pending cleanup for at least `min_age`.
    ///
    /// Call this periodically (e.g., every few minutes) to clean up orphaned
    /// blobs from deleted frames. Use `Duration::ZERO` in tests to sweep
    /// immediately.
    ///
    /// Returns the number of blobs swept.
    pub async fn sweep_blobs(&self, min_age: std::time::Duration) -> Result<usize> {
        self.storage.sweep_blobs(min_age).await
    }

    /// Compact a timeline by replacing consecutive Full frames with Deltas.
    ///
    /// Walks frames in `(timestamp, graph_id)` order within the given range.
    /// The first Full frame stays Full. Every subsequent Full is replaced with
    /// a Delta derived from the previous data-carrying frame. Empty and Error
    /// frames break the chain (the next Full after them stays Full).
    ///
    /// Returns the number of frames converted from Full to Delta.
    pub async fn compact_timeline(
        &self,
        timeline_id: &TimelineID,
        start: Option<Timestamp>,
        end: Option<Timestamp>,
    ) -> Result<usize> {
        crate::adjacent_deltas::compact_timeline(&self.storage, timeline_id, start, end).await
    }
}

// -- Private helpers for external ID allocation --

/// Load existing mappings, validate linear history, and insert new ones.
/// Must be called inside an exclusive transaction.
async fn resolve_and_allocate(
    conn: &dyn UnigraphGraphConnection,
    ns: &ExternalIDNamespace,
    external_ids: &[ExternalID],
) -> Result<Vec<GraphID>> {
    let existing = conn.list_external_id_mappings(ns).await?;
    let existing_map: HashMap<&str, GraphID> = existing
        .iter()
        .map(|(eid, gid)| (eid.0.as_str(), *gid))
        .collect();

    let skip_count = count_overlap_prefix(external_ids, &existing_map);
    validate_no_gaps(external_ids, skip_count, &existing_map)?;

    let prefix_ids = resolve_prefix(external_ids, skip_count, &existing_map);
    let new_mappings = allocate_new_tail(external_ids, skip_count, &existing);
    conn.insert_external_id_mappings(ns, &new_mappings).await?;

    let new_ids: Vec<GraphID> = new_mappings.into_iter().map(|(_, gid)| gid).collect();
    Ok([prefix_ids, new_ids].concat())
}

/// Commit on success, rollback on error, release lock in both cases.
async fn finish_transaction<T>(
    conn: &dyn UnigraphGraphConnection,
    result: Result<T>,
    lock_name: &str,
) -> Result<T> {
    match result {
        Ok(val) => {
            conn.commit_transaction().await?;
            conn.release_named_lock(lock_name).await?;
            Ok(val)
        }
        Err(e) => {
            // Transaction rolls back on connection drop.
            conn.release_named_lock(lock_name).await?;
            Err(e)
        }
    }
}

/// Count how many external_ids from the front already exist (contiguous prefix).
fn count_overlap_prefix(external_ids: &[ExternalID], existing: &HashMap<&str, GraphID>) -> usize {
    external_ids
        .iter()
        .take_while(|eid| existing.contains_key(eid.0.as_str()))
        .count()
}

/// Verify no external_id after the overlap prefix already exists.
fn validate_no_gaps(
    external_ids: &[ExternalID],
    skip_count: usize,
    existing: &HashMap<&str, GraphID>,
) -> Result<()> {
    for eid in &external_ids[skip_count..] {
        if existing.contains_key(eid.0.as_str()) {
            anyhow::bail!(
                "Non-linear history: external_id '{}' already exists but appears after \
                 new external_ids in the input list. The overlapping prefix must be contiguous.",
                eid.0
            );
        }
    }
    Ok(())
}

/// Resolve the overlapping prefix to their existing GraphIDs.
fn resolve_prefix(
    external_ids: &[ExternalID],
    skip_count: usize,
    existing: &HashMap<&str, GraphID>,
) -> Vec<GraphID> {
    external_ids[..skip_count]
        .iter()
        .map(|eid| existing[eid.0.as_str()])
        .collect()
}

/// Build (ExternalID, GraphID) pairs for the new tail, starting after the
/// highest existing graph_id.
fn allocate_new_tail(
    external_ids: &[ExternalID],
    skip_count: usize,
    existing: &[(ExternalID, GraphID)],
) -> Vec<(ExternalID, GraphID)> {
    let mut next_id = existing.last().map(|(_, gid)| gid.0).unwrap_or(0) + 1;
    external_ids[skip_count..]
        .iter()
        .map(|eid| {
            let gid = GraphID(next_id);
            next_id += 1;
            (eid.clone(), gid)
        })
        .collect()
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Storage trait definitions for the Unigraph storage layer.
//!
//! [`UnigraphGraphStorage`] is the top-level storage backend that vends
//! [`UnigraphGraphConnection`]s. All data operations happen through connections.
//! [`UnigraphBlobStorage`] handles external blob storage for large payloads.

use anyhow::Result;
use async_trait::async_trait;

use crate::frame::FrameRow;
use crate::types::ExternalID;
use crate::types::ExternalIDNamespace;
use crate::types::FrameQuery;
use crate::types::FrameType;
use crate::types::GraphID;
use crate::types::GraphKey;
use crate::types::GraphTimeKey;
use crate::types::TimelineConfig;
use crate::types::TimelineID;
use crate::types::Timestamp;

/// A connection to the graph storage.
///
/// All data operations go through a connection. Connections also provide
/// transaction control: call [`start_transaction`](Self::start_transaction)
/// to begin a transaction and [`commit_transaction`](Self::commit_transaction)
/// to commit it. If the connection is dropped while a transaction is active
/// (i.e. started but not committed), the transaction is rolled back.
#[async_trait]
pub trait UnigraphGraphConnection: Send + Sync {
    /// Begin a transaction. All subsequent operations on this connection
    /// will be part of the transaction until [`commit_transaction`](Self::commit_transaction)
    /// is called or the connection is dropped (which rolls back).
    async fn start_transaction(&self) -> Result<()>;

    /// Commit the active transaction.
    async fn commit_transaction(&self) -> Result<()>;

    /// Create a new timeline with the given configuration.
    async fn create_timeline(
        &self,
        timeline_id: &TimelineID,
        config: &TimelineConfig,
    ) -> Result<()>;

    /// Get the configuration for an existing timeline.
    /// Returns `None` if the timeline does not exist.
    async fn get_timeline_config(&self, timeline_id: &TimelineID)
    -> Result<Option<TimelineConfig>>;

    /// Get the timeline configuration and acquire an exclusive lock on it.
    ///
    /// This ensures only one process can store graphs to a timeline at a time.
    /// The lock is held until the transaction is committed or the connection
    /// is dropped (which rolls back).
    ///
    /// **Caller must already be inside a transaction** (via [`start_transaction`]).
    ///
    /// - SQLite: just reads the config — `BEGIN EXCLUSIVE` already serializes writers.
    /// - MySQL: uses `SELECT ... FOR UPDATE` to lock the timeline row.
    ///
    /// Returns `None` if the timeline does not exist.
    async fn get_timeline_config_and_lock(
        &self,
        timeline_id: &TimelineID,
    ) -> Result<Option<TimelineConfig>>;

    /// List all timeline IDs.
    async fn list_timelines(&self) -> Result<Vec<TimelineID>>;

    /// Store a frame with data (Full, Delta, or Error).
    ///
    /// - `key`: timeline + timestamp + graph ID
    /// - `frame_type`: the type of frame (should not be `Empty` — use [`store_frame_empty`] for that)
    /// - `base`: for Delta frames, the base graph this delta derives from; `None` otherwise
    /// - `manifest_json`: JSON-serialized manifest
    /// - `inline_blobs`: optional ZSTD-compressed blob map (when blobs are small enough to inline)
    async fn store_frame(
        &self,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
    ) -> Result<()>;

    /// Store an empty frame (placeholder with no data).
    async fn store_frame_empty(&self, key: &GraphTimeKey) -> Result<()>;

    /// Select frames matching a structured query.
    ///
    /// The implementation compiles the [`FrameQuery`] into a single SQL
    /// statement with conditional WHERE clauses, ORDER BY, and LIMIT.
    async fn select_frames(&self, query: &FrameQuery) -> Result<Vec<FrameRow>>;

    /// Delete a frame row by its graph key.
    ///
    /// Only deletes the database row (metadata + inline blobs). Does NOT
    /// touch external blob storage — the caller is responsible for
    /// registering external blob keys for cleanup before calling this.
    ///
    /// Returns `true` if a frame was deleted, `false` if it didn't exist.
    async fn delete_frame(&self, key: &GraphKey) -> Result<bool>;

    /// Register blob keys for deferred cleanup.
    ///
    /// Used during store operations: if the transaction fails after blobs
    /// have been uploaded to external storage, these keys can be cleaned up later.
    async fn register_blobs_for_cleanup(&self, blob_keys: &[String]) -> Result<()>;

    /// Unregister blob keys from the cleanup list (transaction succeeded).
    async fn unregister_blobs_for_cleanup(&self, blob_keys: &[String]) -> Result<()>;

    /// Get all blob keys that are pending cleanup.
    async fn get_blobs_pending_cleanup(&self) -> Result<Vec<String>>;

    /// Get blob keys pending cleanup that were registered before `older_than`.
    ///
    /// Only returns entries whose `created_at` is strictly before `older_than`.
    /// This ensures recently-registered blobs (from in-flight transactions)
    /// are not swept prematurely.
    async fn get_blobs_pending_cleanup_older_than(
        &self,
        older_than: Timestamp,
    ) -> Result<Vec<String>>;

    // -- Named locks --

    /// Acquire a named advisory lock.
    ///
    /// Used to ensure only one process at a time can run a particular operation
    /// (e.g. ingestion for a specific namespace). The lock is automatically
    /// released when the connection is dropped.
    ///
    /// - SQLite: no-op (`BEGIN EXCLUSIVE` in `start_transaction` already serializes writers)
    /// - MySQL: `GET_LOCK(name, timeout)`, released on connection drop
    async fn acquire_named_lock(&self, name: &str) -> Result<()>;

    /// Release a named advisory lock.
    ///
    /// - SQLite: no-op
    /// - MySQL: `RELEASE_LOCK(name)`
    async fn release_named_lock(&self, name: &str) -> Result<()>;

    // -- External ID mappings --

    /// Load all external ID mappings for a namespace, ordered by graph_id ASC.
    async fn list_external_id_mappings(
        &self,
        ns: &ExternalIDNamespace,
    ) -> Result<Vec<(ExternalID, GraphID)>>;

    /// Insert a batch of new external ID → graph ID mappings.
    /// Caller is responsible for transaction management.
    async fn insert_external_id_mappings(
        &self,
        ns: &ExternalIDNamespace,
        mappings: &[(ExternalID, GraphID)],
    ) -> Result<()>;

    /// Look up the ExternalID for a GraphID within a namespace.
    async fn graph_id_to_external_id(
        &self,
        external_id_namespace: &ExternalIDNamespace,
        graph_id: &GraphID,
    ) -> Result<Option<ExternalID>>;

    /// Look up ExternalIDs for multiple GraphIDs within a namespace (batch).
    async fn graph_ids_to_external_ids(
        &self,
        external_id_namespace: &ExternalIDNamespace,
        graph_ids: &[GraphID],
    ) -> Result<Vec<(GraphID, ExternalID)>>;

    /// Get the ExternalID with the highest GraphID in a namespace.
    ///
    /// Used for incremental ingestion: find the last-known external ID
    /// to query the source system for only newer entries.
    async fn get_latest_external_id(
        &self,
        external_id_namespace: &ExternalIDNamespace,
    ) -> Result<Option<ExternalID>>;

    // -- Metric history --

    /// Ensure metric_history rows exist for the given `(timeline, week, node_name)` combos.
    ///
    /// Uses `INSERT OR IGNORE` semantics with empty placeholder data.
    /// **Must be called BEFORE the transaction** — MySQL has a 15-year-old bug
    /// where it gives away multiple locks for the same non-existent row within
    /// a transaction.
    async fn ensure_metric_history_partitions_exist(
        &self,
        timeline_id: &TimelineID,
        week_key: &str,
        node_names: &[String],
    ) -> Result<()>;

    /// Batch-fetch all metric history blobs for a timeline + ISO week.
    ///
    /// Returns all node blobs for the given week. Within a transaction,
    /// this effectively locks these rows.
    async fn get_metric_history_for_week(
        &self,
        timeline_id: &TimelineID,
        week_key: &str,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>>;

    /// Batch-upsert metric history blobs for a timeline + ISO week.
    ///
    /// `INSERT OR REPLACE` semantics — replaces the blob for each
    /// `(timeline, node_name, week)` tuple.
    async fn upsert_metric_history_batch(
        &self,
        timeline_id: &TimelineID,
        week_key: &str,
        entries: &[(String, Vec<u8>)],
    ) -> Result<()>;

    /// Fetch metric history blobs for specific nodes within a week range.
    ///
    /// Returns `(node_name, week_key, compressed_blob)` tuples, ordered by
    /// `(node_name, week_key)`. `start_week` and `end_week` are inclusive
    /// bounds in `"YYYY-Www"` format.
    async fn get_metric_history_range(
        &self,
        timeline_id: &TimelineID,
        node_names: &[String],
        start_week: &str,
        end_week: &str,
    ) -> Result<Vec<(String, String, Vec<u8>)>>;
}

/// Graph storage backend — vends connections.
///
/// Implementations manage the underlying connection resource (e.g. a mutex-guarded
/// SQLite connection, a MySQL connection pool). Each call to [`conn`](Self::conn)
/// returns a connection that holds whatever resources are needed (lock guard,
/// pooled connection, etc.) for the duration of its lifetime.
#[async_trait]
pub trait UnigraphGraphStorage: Send + Sync {
    /// Get a connection to the storage.
    ///
    /// For SQLite, this acquires the mutex and returns a connection that holds
    /// the lock guard. For connection-pooled backends, this checks out a
    /// connection from the pool.
    async fn conn(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>>;
}

/// External blob storage trait — manages large blobs outside the frames table.
///
/// Used when the total blob size exceeds [`INLINE_BLOB_THRESHOLD_BYTES`](crate::types::INLINE_BLOB_THRESHOLD_BYTES).
#[async_trait]
pub trait UnigraphBlobStorage: Send + Sync {
    /// Store a blob by key.
    async fn put_blob(&self, key: &str, data: &[u8]) -> Result<()>;

    /// Retrieve a blob by key. Returns `None` if not found.
    async fn get_blob(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Delete a blob by key.
    async fn delete_blob(&self, key: &str) -> Result<()>;

    /// Check if a blob exists.
    async fn has_blob(&self, key: &str) -> Result<bool>;

    /// List all blob keys matching a prefix.
    async fn list_blobs(&self, prefix: &str) -> Result<Vec<String>>;
}

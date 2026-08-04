// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Storage trait definitions for the Unigraph storage layer.
//!
//! [`UnigraphGraphStorage`] is the top-level storage backend that vends
//! [`UnigraphGraphConnection`]s. All data operations happen through connections.
//! [`UnigraphBlobStorage`] handles external blob storage for large payloads.

use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use ll;
use unigraph_core::config_key::ConfigRow;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;

use crate::frame::FrameRow;
use crate::history::HistoryEntryRow;
use crate::history::HistoryRange;
use crate::history::HistoryStatusRow;
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

/// Inclusive `(lower, upper)` `GraphID` bounds; `None` on either side means unbounded.
pub type GraphIDBounds = (Option<GraphID>, Option<GraphID>);

/// A connection to the graph storage.
///
/// All data operations go through a connection. Connections also provide
/// transaction control: call [`start_transaction`](Self::start_transaction)
/// to begin a transaction and [`commit_transaction`](Self::commit_transaction)
/// to commit it. If the connection is dropped while a transaction is active
/// (i.e. started but not committed), the transaction is rolled back.
#[async_trait]
pub trait UnigraphGraphConnection: Send {
    /// Begin a transaction. All subsequent operations on this connection
    /// will be part of the transaction until [`commit_transaction`](Self::commit_transaction)
    /// is called or the connection is dropped (which rolls back).
    async fn start_transaction(&mut self, task: &ll::Task) -> Result<()>;

    /// Commit the active transaction.
    async fn commit_transaction(&mut self, task: &ll::Task) -> Result<()>;

    /// Create a new timeline with the given configuration.
    async fn create_timeline(
        &mut self,
        timeline_id: &TimelineID,
        config: &TimelineConfig,
        task: &ll::Task,
    ) -> Result<()>;

    /// Get the configuration for an existing timeline.
    /// Returns `None` if the timeline does not exist.
    async fn get_timeline_config(
        &mut self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<Option<TimelineConfig>>;

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
        &mut self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<Option<TimelineConfig>>;

    /// List all timeline IDs.
    async fn list_timelines(&mut self, task: &ll::Task) -> Result<Vec<TimelineID>>;

    /// Store a frame with data (Full, Delta, or Error).
    ///
    /// - `key`: timeline + timestamp + graph ID
    /// - `frame_type`: the type of frame (should not be `Empty` — use [`store_frames_empty`] for that)
    /// - `base`: for Delta frames, the base graph this delta derives from; `None` otherwise
    /// - `manifest_json`: JSON-serialized manifest
    /// - `inline_blobs`: optional ZSTD-compressed blob map (when blobs are small enough to inline)
    /// - `expires_at`: optional expiration timestamp; `None` means the frame never expires
    #[allow(clippy::too_many_arguments)]
    async fn store_frame(
        &mut self,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()>;

    /// Store empty frames (placeholders with no data) in a batch.
    async fn store_frames_empty(&mut self, keys: &[GraphTimeKey], task: &ll::Task) -> Result<()>;

    /// Select frames matching a structured query.
    ///
    /// The implementation compiles the [`FrameQuery`] into a single SQL
    /// statement with conditional WHERE clauses, ORDER BY, and LIMIT.
    async fn select_frames(&mut self, query: &FrameQuery, task: &ll::Task)
    -> Result<Vec<FrameRow>>;

    /// Delete a frame row by its graph key.
    ///
    /// Only deletes the database row (metadata + inline blobs). Does NOT
    /// touch external blob storage — the caller is responsible for
    /// registering external blob keys for cleanup before calling this.
    ///
    /// Returns `true` if a frame was deleted, `false` if it didn't exist.
    async fn delete_frame(&mut self, key: &GraphKey, task: &ll::Task) -> Result<bool>;

    /// Register blob keys for deferred cleanup.
    ///
    /// Used during store operations: if the transaction fails after blobs
    /// have been uploaded to external storage, these keys can be cleaned up later.
    async fn register_blobs_for_cleanup(
        &mut self,
        blob_keys: &[String],
        task: &ll::Task,
    ) -> Result<()>;

    /// Unregister blob keys from the cleanup list (transaction succeeded).
    async fn unregister_blobs_for_cleanup(
        &mut self,
        blob_keys: &[String],
        task: &ll::Task,
    ) -> Result<()>;

    /// Get all blob keys that are pending cleanup.
    async fn get_blobs_pending_cleanup(&mut self, task: &ll::Task) -> Result<Vec<String>>;

    /// Get blob keys pending cleanup that were registered before `older_than`.
    ///
    /// Only returns entries whose `created_at` is strictly before `older_than`.
    /// This ensures recently-registered blobs (from in-flight transactions)
    /// are not swept prematurely.
    ///
    /// Results are ordered newest-registered first. `limit` caps the number of
    /// keys returned (`None` = no cap); a bounded batch lets callers drain a
    /// large backlog incrementally without doing unbounded work at once.
    /// Newest-first ensures that a persistently-failing old blob can't block
    /// cleanup of freshly-registered ones when a limit is in effect.
    async fn get_blobs_pending_cleanup_older_than(
        &mut self,
        older_than: Timestamp,
        limit: Option<i64>,
        task: &ll::Task,
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
    async fn acquire_named_lock(&mut self, name: &str, task: &ll::Task) -> Result<()>;

    /// Release a named advisory lock.
    ///
    /// - SQLite: no-op
    /// - MySQL: `RELEASE_LOCK(name)`
    async fn release_named_lock(&mut self, name: &str, task: &ll::Task) -> Result<()>;

    // -- External ID mappings --

    /// Load all external ID mappings for a namespace, ordered by graph_id ASC.
    async fn list_external_id_mappings(
        &mut self,
        ns: &ExternalIDNamespace,
        task: &ll::Task,
    ) -> Result<Vec<(ExternalID, GraphID)>>;

    /// Insert a batch of new external ID → graph ID mappings.
    /// Caller is responsible for transaction management.
    async fn insert_external_id_mappings(
        &mut self,
        ns: &ExternalIDNamespace,
        mappings: &[(ExternalID, GraphID)],
        task: &ll::Task,
    ) -> Result<()>;

    /// Look up the ExternalID for a GraphID within a namespace.
    async fn graph_id_to_external_id(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
        graph_id: &GraphID,
        task: &ll::Task,
    ) -> Result<Option<ExternalID>>;

    /// Look up ExternalIDs for multiple GraphIDs within a namespace (batch).
    async fn graph_ids_to_external_ids(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
        graph_ids: &[GraphID],
        task: &ll::Task,
    ) -> Result<Vec<(GraphID, ExternalID)>>;

    /// Get the ExternalID with the highest GraphID in a namespace.
    ///
    /// Used for incremental ingestion: find the last-known external ID
    /// to query the source system for only newer entries.
    async fn get_latest_external_id(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
        task: &ll::Task,
    ) -> Result<Option<ExternalID>>;

    // -- Metric history --

    /// Ensure metric_history rows exist for the given `(timeline, week, node_name)` combos.
    ///
    /// Uses `INSERT OR IGNORE` semantics with empty placeholder data.
    /// **Must be called BEFORE the transaction** — MySQL has a 15-year-old bug
    /// where it gives away multiple locks for the same non-existent row within
    /// a transaction.
    async fn ensure_metric_history_partitions_exist(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
        node_names: &[String],
        task: &ll::Task,
    ) -> Result<()>;

    /// Batch-fetch all metric history blobs for a timeline + ISO week.
    ///
    /// Returns all node blobs for the given week. Within a transaction,
    /// this effectively locks these rows.
    async fn get_metric_history_for_week(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
        task: &ll::Task,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>>;

    /// Batch-upsert metric history blobs for a timeline + ISO week.
    ///
    /// `INSERT OR REPLACE` semantics — replaces the blob for each
    /// `(timeline, node_name, week)` tuple.
    async fn upsert_metric_history_batch(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
        entries: &[(String, Vec<u8>)],
        task: &ll::Task,
    ) -> Result<()>;

    /// Fetch metric history blobs for specific nodes within a week range.
    ///
    /// Returns `(node_name, week_key, compressed_blob)` tuples, ordered by
    /// `(node_name, week_key)`. `start_week` and `end_week` are inclusive
    /// bounds in `"YYYY-Www"` format.
    async fn get_metric_history_range(
        &mut self,
        timeline_id: &TimelineID,
        node_names: &[String],
        start_week: &str,
        end_week: &str,
        task: &ll::Task,
    ) -> Result<Vec<(String, String, Vec<u8>)>>;

    // -- Graph history (plain rows) --
    //
    // Thin SQL wrappers over the `graph_history_*` tables. No locking, no
    // transactions, no business logic — the `GraphHistory` namespace in
    // `unigraph_db` owns all of that.

    /// Intern metric names into the per-timeline dictionary and return the
    /// timeline's full `name -> metric_id` mapping.
    ///
    /// Append-only: existing names keep their ids so previously written value
    /// blobs stay decodable. New names get `MAX(metric_id) + 1` per timeline.
    async fn intern_history_metrics(
        &mut self,
        timeline_id: &TimelineID,
        names: &[String],
        task: &ll::Task,
    ) -> Result<BTreeMap<String, u32>>;

    /// Read the per-timeline `metric_id -> name` dictionary (for labeling reads).
    async fn get_history_metric_names(
        &mut self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<BTreeMap<u32, String>>;

    /// Insert history entries with `INSERT OR REPLACE` semantics.
    async fn insert_history_entries(
        &mut self,
        timeline_id: &TimelineID,
        rows: &[HistoryEntryRow],
        task: &ll::Task,
    ) -> Result<()>;

    /// For each requested node, the most recent *baseline* value blob strictly
    /// before `before_graph_id`. Nodes with no earlier entry are omitted.
    ///
    /// Deferred rows are skipped: they are provisional and compaction will
    /// remove them, so measuring a later sample against one would hide the
    /// drift accumulated since the last surviving sample.
    async fn get_last_history_entries_before(
        &mut self,
        timeline_id: &TimelineID,
        before_graph_id: GraphID,
        node_names: &[String],
        task: &ll::Task,
    ) -> Result<Vec<(String, Vec<u8>)>>;

    /// One node's kept series within `range`, ordered by `graph_id` ascending.
    async fn get_history_series(
        &mut self,
        timeline_id: &TimelineID,
        node_name: &str,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<Vec<(GraphID, Timestamp, Vec<u8>)>>;

    /// Distinct node names that have history entries within `range`.
    async fn list_history_node_names(
        &mut self,
        timeline_id: &TimelineID,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<Vec<String>>;

    /// Batch-read ingest checkpoints. Graph IDs with no row are omitted.
    async fn get_history_status(
        &mut self,
        timeline_id: &TimelineID,
        graph_ids: &[GraphID],
        task: &ll::Task,
    ) -> Result<Vec<HistoryStatusRow>>;

    /// Batch-write ingest checkpoints with `INSERT OR REPLACE` semantics.
    async fn upsert_history_status(
        &mut self,
        timeline_id: &TimelineID,
        rows: &[HistoryStatusRow],
        task: &ll::Task,
    ) -> Result<()>;

    /// Graph-ID span of the checkpoints still flagged `omission_deferred`, or
    /// `None` when there are none. This is `history compact`'s work list — the
    /// ranges ingested across a gap that still need their threshold re-applied.
    async fn get_history_deferred_bounds(
        &mut self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<Option<GraphIDBounds>>;

    /// Clear the `omission_deferred` flag within `bounds`. Returns the row count.
    async fn clear_history_omission_deferred(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Graph IDs still flagged `omission_deferred` within `bounds`, ascending.
    ///
    /// This is the per-frame work list. Ascending order is required: deciding
    /// a frame's rows can delete them, which moves the baseline the next
    /// frame's rows are measured against.
    async fn list_history_deferred_graph_ids(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<Vec<GraphID>>;

    /// Every `(node_name, values)` row recorded at one graph ID.
    ///
    /// A flagged frame took *all* of its verdicts against an untrustworthy
    /// baseline — it could keep a row the settled chain would drop just as
    /// easily as the reverse — so compaction reconsiders the whole frame, not
    /// only the rows it had to defer.
    async fn get_history_entries_at(
        &mut self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<Vec<(String, Vec<u8>)>>;

    /// Delete the given nodes' entries at one graph ID. Returns the row count.
    async fn delete_history_entries_at(
        &mut self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        node_names: &[String],
        task: &ll::Task,
    ) -> Result<u64>;

    /// Clear the per-entry `deferred` flag within `bounds`. Returns the row count.
    ///
    /// Call only on a range that has just been compacted: every row still
    /// present there survived the threshold, so it is a baseline row now and
    /// must become visible to `get_last_history_entries_before`.
    async fn clear_history_entries_deferred(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Non-null `error_blob_key`s recorded within `bounds` (for cleanup registration).
    async fn get_history_error_blob_keys(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<Vec<String>>;

    /// Delete all history entries within `bounds`. Returns the row count.
    async fn delete_history_entries(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Delete one node's history entries at the given graph IDs. Returns the row count.
    async fn delete_history_entries_for_node(
        &mut self,
        timeline_id: &TimelineID,
        node_name: &str,
        graph_ids: &[GraphID],
        task: &ll::Task,
    ) -> Result<u64>;

    /// Delete ingest checkpoints within `bounds`. Returns the row count.
    async fn delete_history_status(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Drop the timeline's metric-name dictionary. Returns the row count.
    ///
    /// Only safe once every entry for the timeline is gone — ids are never
    /// reused, so a surviving blob would decode against the wrong names.
    async fn delete_history_metrics(
        &mut self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<u64>;

    // -- Config storage --

    /// Store a traversal config blob by its content-addressed key.
    ///
    /// Uses `INSERT OR IGNORE` semantics — if the key already exists, the write
    /// is silently skipped (deduplication).
    async fn store_traversal_config(
        &mut self,
        key: &TraversalConfigKey,
        blob_inline: Option<&[u8]>,
        blob_id: Option<&str>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()>;

    /// Fetch a traversal config row by key. Returns `None` if not found.
    async fn get_traversal_config(
        &mut self,
        key: &TraversalConfigKey,
        task: &ll::Task,
    ) -> Result<Option<ConfigRow<TraversalConfigKey>>>;

    /// Store a graph query config blob by its content-addressed key.
    async fn store_graph_query_config(
        &mut self,
        key: &GraphQueryConfigKey,
        blob_inline: Option<&[u8]>,
        blob_id: Option<&str>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()>;

    /// Fetch a graph query config row by key. Returns `None` if not found.
    async fn get_graph_query_config(
        &mut self,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<Option<ConfigRow<GraphQueryConfigKey>>>;

    // -- TTL / expiration --

    /// Select config keys that have expired (expires_at <= now).
    async fn select_expired_config_keys(
        &mut self,
        now: Timestamp,
        limit: i64,
        task: &ll::Task,
    ) -> Result<Vec<String>>;

    /// Delete a config row by key.
    ///
    /// Only deletes the database row. Does NOT clean up blobs that were
    /// offloaded to external blob storage via `blob_id` — the caller is
    /// responsible for registering those blob keys for cleanup before
    /// calling this.
    ///
    /// Returns `true` if a config was deleted, `false` if it didn't exist.
    async fn delete_config_db_rows(&mut self, key: &str, task: &ll::Task) -> Result<bool>;

    // ── Unique ID generation ────────────────────────────────────────

    /// Generate a globally unique integer ID.
    ///
    /// Uses an auto-increment table to produce IDs that are unique across
    /// all callers sharing the same database. Connection-scoped `last_insert_id`
    /// makes this safe for concurrent use.
    async fn gen_uniq_id(&mut self, task: &ll::Task) -> Result<i64>;
}

/// Graph storage backend — vends connections.
///
/// Implementations manage the underlying connection resource (e.g. a mutex-guarded
/// SQLite connection, a MySQL connection pool). Each call to [`conn`](Self::conn)
/// returns a connection that holds whatever resources are needed (lock guard,
/// pooled connection, etc.) for the duration of its lifetime.
///
/// # Connection roles
///
/// In addition to the general-purpose [`conn`](Self::conn), role-specific methods
/// let backends route to different connection pools or replicas:
///
/// - [`conn_read`](Self::conn_read) — read-only queries (can go to a replica)
/// - [`conn_write`](Self::conn_write) — read-write operations (primary)
/// - [`conn_master`](Self::conn_master) — admin / DDL operations (primary, may bypass query routing)
/// - [`conn_analytics`](Self::conn_analytics) — heavy analytical queries (dedicated pool / replica)
///
/// All role methods have default implementations that delegate to [`conn`](Self::conn),
/// so single-connection backends (e.g. SQLite) work without overriding anything.
#[async_trait]
pub trait UnigraphGraphStorage: Send + Sync {
    /// Get a general-purpose connection to the storage.
    ///
    /// For SQLite, this acquires the mutex and returns a connection that holds
    /// the lock guard. For connection-pooled backends, this checks out a
    /// connection from the pool.
    async fn conn(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>>;

    /// Get a read-only connection (may route to a replica).
    async fn conn_read(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.conn().await
    }

    /// Get a read-write connection (routes to the primary).
    async fn conn_write(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.conn().await
    }

    /// Get an admin / DDL connection (routes to the primary, may bypass query routing).
    async fn conn_master(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.conn().await
    }

    /// Get a connection for heavy analytical queries (may route to a dedicated pool / replica).
    async fn conn_analytics(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.conn().await
    }
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

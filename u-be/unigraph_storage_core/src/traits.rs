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
use crate::history::ExclusiveGraphIDRange;
use crate::history::FrameFlags;
use crate::history::HistoryEntryRow;
use crate::history::HistoryNodeSample;
use crate::history::HistoryRange;
use crate::history::HistorySampleRow;
use crate::history::HistoryStatusRow;
use crate::history::IngestState;
use crate::history::Reasons;
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

    /// Delete a timeline's configuration row.
    ///
    /// Only the one row. Frames, history, metric history and external ID
    /// mappings all outlive it — deleting the config first would strand them,
    /// since every one of those deletes locks this row. Call it last.
    ///
    /// Returns `true` if a row was deleted, `false` if the timeline didn't exist.
    async fn delete_timeline(&mut self, timeline_id: &TimelineID, task: &ll::Task) -> Result<bool>;

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
    ///
    /// Which payload columns come back is decided by the query's
    /// `with_manifest` / `with_data` flags — see [`FrameRow`].
    async fn select_frames(&mut self, query: &FrameQuery, task: &ll::Task)
    -> Result<Vec<FrameRow>>;

    /// Count a timeline's frames.
    ///
    /// One indexed range scan over the `(timeline_id, graph_id)` primary key.
    /// Cheap enough to run once before a long job that wants a denominator for
    /// its progress bar, not cheap enough to call in a loop.
    async fn count_frames(&mut self, timeline_id: &TimelineID, task: &ll::Task) -> Result<i64>;

    /// Delete a frame row by its graph key.
    ///
    /// Only deletes the database row (metadata + inline blobs). Does NOT
    /// touch external blob storage — the caller is responsible for
    /// registering external blob keys for cleanup before calling this.
    ///
    /// Returns `true` if a frame was deleted, `false` if it didn't exist.
    async fn delete_frame(&mut self, key: &GraphKey, task: &ll::Task) -> Result<bool>;

    /// Delete the named frames of `timeline_id` in one statement. Returns the
    /// row count.
    ///
    /// Same contract as [`delete_frame`](Self::delete_frame) on blobs: rows
    /// only. The caller registers those frames' external blob keys for cleanup
    /// first, inside the same transaction.
    ///
    /// Takes an explicit list rather than a `graph_id` range on purpose. A bulk
    /// caller gets its batch from `select_frames`, which orders by
    /// `(timestamp, graph_id)` — on a timeline written out of order those are
    /// different orders, so the batch's `graph_id` span can cover frames the
    /// batch never saw, and deleting by span would drop them without ever
    /// registering their blobs.
    async fn delete_frames(
        &mut self,
        timeline_id: &TimelineID,
        graph_ids: &[GraphID],
        task: &ll::Task,
    ) -> Result<u64>;

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

    /// Delete every mapping in a namespace. Returns the row count.
    ///
    /// Allocation is sequential from the highest existing `GraphID`, so this
    /// resets the namespace's counter to zero. Only safe once nothing refers to
    /// those IDs any more.
    async fn delete_external_id_mappings(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
        task: &ll::Task,
    ) -> Result<u64>;

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

    /// List the distinct ISO weeks a timeline has metric history for, ascending.
    ///
    /// The unit a bulk delete works in: rows are `nodes x weeks`, and the week
    /// is the only column with an index that can bound the delete.
    async fn list_metric_history_weeks(
        &mut self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<Vec<String>>;

    /// Delete a timeline's metric history for one ISO week. Returns the row count.
    async fn delete_metric_history_for_week(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
        task: &ll::Task,
    ) -> Result<u64>;

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
    ///
    /// `reasons` is overwritten, not OR-ed: re-ingesting a frame in its own
    /// right supersedes whatever a neighbour minted for it. Callers that mean
    /// to *add* a reason to an existing row use [`Self::set_history_reasons_at`].
    async fn insert_history_entries(
        &mut self,
        timeline_id: &TimelineID,
        rows: &[HistoryEntryRow],
        task: &ll::Task,
    ) -> Result<()>;

    /// One node's stored series within `range`, ordered by `graph_id` ascending.
    async fn get_history_series(
        &mut self,
        timeline_id: &TimelineID,
        node_name: &str,
        range: &HistoryRange,
        task: &ll::Task,
    ) -> Result<Vec<HistorySampleRow>>;

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

    /// Every ingest checkpoint within `bounds`, ascending by `graph_id`.
    ///
    /// Both `ingest` and `compact` need the whole checkpoint sequence rather
    /// than a lookup by id: the work list is "everything not yet `Ingested`,
    /// with no time bound", and gap flags are a function of the sequence around
    /// each frame. One scan answers both.
    async fn list_history_statuses(
        &mut self,
        timeline_id: &TimelineID,
        bounds: &GraphIDBounds,
        task: &ll::Task,
    ) -> Result<Vec<HistoryStatusRow>>;

    /// Batch-write ingest checkpoints with `INSERT OR REPLACE` semantics.
    async fn upsert_history_status(
        &mut self,
        timeline_id: &TimelineID,
        rows: &[HistoryStatusRow],
        task: &ll::Task,
    ) -> Result<()>;

    /// Overwrite `frame_flags` for the given frames, leaving every other
    /// checkpoint column alone. Returns the row count.
    ///
    /// Separate from [`Self::upsert_history_status`] because gap structure
    /// changes for reasons that have nothing to do with ingest progress — a
    /// placeholder appearing past the head of the timeline restates its
    /// neighbour's flags without touching what history has done with it.
    async fn set_history_frame_flags(
        &mut self,
        timeline_id: &TimelineID,
        rows: &[(GraphID, FrameFlags)],
        task: &ll::Task,
    ) -> Result<u64>;

    /// Move the given frames to `ingest_state`, leaving every other checkpoint
    /// column alone. Returns the row count.
    ///
    /// This is how a frame is handed *back* to ingest. A frame that has stopped
    /// being the far edge of a gap holds rows that were never judged, and
    /// judging them needs the predecessor's values — that is, a graph. Marking
    /// it `Pending` puts it back on the one work list instead of inventing a
    /// second.
    async fn set_history_ingest_states(
        &mut self,
        timeline_id: &TimelineID,
        graph_ids: &[GraphID],
        ingest_state: IngestState,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Every node's row recorded at one graph ID.
    async fn get_history_entries_at(
        &mut self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<Vec<HistoryNodeSample>>;

    /// Delete the given nodes' entries at one graph ID. Returns the row count.
    async fn delete_history_entries_at(
        &mut self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        node_names: &[String],
        task: &ll::Task,
    ) -> Result<u64>;

    /// OR in `set` and mask out `clear` on the given nodes' rows at one graph
    /// ID. Returns the row count.
    ///
    /// Read-modify-write in SQL rather than in Rust, because the common caller
    /// is ingest adding `ANCHOR` to rows it did not write and must not clobber:
    /// the row it is flagging may be a threshold crossing in its own right, and
    /// under this design those two coexist.
    async fn set_history_reasons_at(
        &mut self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        node_names: &[String],
        set: Reasons,
        clear: Reasons,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Mask out `clear` on **every** row at one graph ID. Returns the row count.
    ///
    /// The node-list-free variant, for retiring a whole-frame reason such as
    /// `LATEST` when a newer frame takes over. Enumerating the node names would
    /// mean shipping the entire graph's node set to say "all of them".
    async fn clear_history_reasons_at(
        &mut self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        clear: Reasons,
        task: &ll::Task,
    ) -> Result<u64>;

    /// Overwrite one node's `reasons` at the given graph IDs. Returns the row
    /// count.
    ///
    /// Compaction's write: it re-derives a whole series at once and each row
    /// lands on its own answer, so this sets exact values rather than
    /// OR-ing bits.
    async fn set_history_entry_reasons(
        &mut self,
        timeline_id: &TimelineID,
        node_name: &str,
        rows: &[(GraphID, Reasons)],
        task: &ll::Task,
    ) -> Result<u64>;

    /// Delete every zero-reason row strictly between the bounds, for all nodes.
    /// Returns the row count.
    ///
    /// The collapse half of compaction. Bounds are exclusive because they are
    /// barrier frames, whose rows are held by their frame flags — so this needs
    /// no join, no node list, and no per-node round trip, just one range
    /// statement per segment however many nodes the timeline has.
    async fn delete_collapsed_history_entries(
        &mut self,
        timeline_id: &TimelineID,
        segment: &ExclusiveGraphIDRange,
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

    /// Retrieve a blob by key. A missing key is an **error**, not an empty
    /// result.
    ///
    /// Nothing reads a blob speculatively: a caller has a key because a
    /// manifest, a config row or an alert row handed it one, so a key that
    /// doesn't resolve means that reference is broken. Returning `Option` here
    /// only moved the error one line down the call stack, made every caller
    /// invent its own wording for it, and threw away the backend's context
    /// (bucket, path, troubleshooting link) on the way. Use
    /// [`has_blob`](Self::has_blob) when the question really is "is it there?".
    async fn get_blob(&self, key: &str) -> Result<Vec<u8>>;

    /// Delete a blob by key.
    ///
    /// **Must be idempotent: deleting a key that isn't there is `Ok(())`,
    /// not an error.** The cleanup queue routinely holds keys with no blob
    /// behind them — the crash-safe store path registers a key before
    /// uploading it, so a store that dies in between leaves a registration for
    /// a blob that was never written. A backend that errors on those never
    /// clears them: the sweep keeps every failed key queued and retries it on
    /// the next pass, so a stale key would be reported as a failure forever.
    ///
    /// It no longer takes the rest of the batch down with it — the sweep
    /// unregisters the keys that did delete regardless — but "permanently
    /// failing, permanently retried" is not a state to design for.
    async fn delete_blob(&self, key: &str) -> Result<()>;

    /// Check if a blob exists.
    async fn has_blob(&self, key: &str) -> Result<bool>;

    /// List all blob keys matching a prefix.
    async fn list_blobs(&self, prefix: &str) -> Result<Vec<String>>;
}

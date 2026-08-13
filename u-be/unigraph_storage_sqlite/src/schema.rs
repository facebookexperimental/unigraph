// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SQL DDL constants and migrations for the SQLite storage backend.

/// Table names
pub const TABLE_TIMELINE_CONFIGS: &str = "timeline_configs";
pub const TABLE_GRAPHS: &str = "graphs";
pub const TABLE_BLOBS: &str = "blobs";
pub const TABLE_BLOBS_TO_DELETE: &str = "blobs_to_delete";
pub const TABLE_EXTERNAL_ID_MAPPINGS: &str = "external_id_mappings";
pub const TABLE_METRIC_HISTORY: &str = "metric_history";
pub const TABLE_GRAPH_HISTORY_METRICS: &str = "graph_history_metrics";
pub const TABLE_GRAPH_HISTORY_STATUS: &str = "graph_history_status";
pub const TABLE_GRAPH_HISTORY_ENTRIES: &str = "graph_history_entries";
pub const TABLE_CONFIGS: &str = "configs";
pub const TABLE_UNIQ_IDS: &str = "uniq_ids";

/// SQL statements to create the storage schema.
///
/// Safe to run multiple times (`IF NOT EXISTS`).
pub const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS timeline_configs (
    timeline_id   TEXT PRIMARY KEY,
    config_json   TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS graphs (
    timeline_id   TEXT    NOT NULL,
    timestamp     INTEGER NOT NULL,
    graph_id      INTEGER NOT NULL,
    frame_type    TEXT    NOT NULL,
    manifest_json TEXT,
    inline_blobs  BLOB,
    base_key_json TEXT,
    created_at    INTEGER NOT NULL,
    expires_at    INTEGER,
    PRIMARY KEY (timeline_id, graph_id)
);

CREATE INDEX IF NOT EXISTS idx_graphs_timeline_ts
    ON graphs(timeline_id, timestamp, graph_id);

CREATE INDEX IF NOT EXISTS idx_graphs_timeline_expires
    ON graphs(timeline_id, expires_at)
    WHERE expires_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS blobs (
    blob_key    TEXT PRIMARY KEY,
    data        BLOB NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS blobs_to_delete (
    blob_key    TEXT PRIMARY KEY,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS external_id_mappings (
    external_id_namespace TEXT    NOT NULL,
    external_id           TEXT    NOT NULL,
    graph_id              INTEGER NOT NULL,
    created_at            INTEGER NOT NULL,
    PRIMARY KEY (external_id_namespace, external_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_external_id_mappings_reverse
    ON external_id_mappings(external_id_namespace, graph_id);

CREATE TABLE IF NOT EXISTS metric_history (
    timeline_id   TEXT    NOT NULL,
    node_name     TEXT    NOT NULL,
    week_key      TEXT    NOT NULL,
    data          BLOB    NOT NULL,
    updated_at    INTEGER NOT NULL,
    PRIMARY KEY (timeline_id, node_name, week_key)
);

CREATE INDEX IF NOT EXISTS idx_metric_history_timeline_week
    ON metric_history(timeline_id, week_key);

-- Per-timeline metric-name dictionary for graph_history_entries.
-- Append-only with stable ids: reordering or removing an id would make every
-- previously-written metric_values blob decode against the wrong names.
CREATE TABLE IF NOT EXISTS graph_history_metrics (
    timeline_id  TEXT    NOT NULL,
    metric_id    INTEGER NOT NULL,
    metric_name  TEXT    NOT NULL,
    PRIMARY KEY (timeline_id, metric_id)
) WITHOUT ROWID;

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_history_metrics_name
    ON graph_history_metrics(timeline_id, metric_name);

-- Per-(timeline, graph_id) ingest checkpoint, and the ingest work list.
--
-- Every frame whose `ingest_state` is not 'Ingested' stays on that list with
-- no time bound, which is the whole recovery story: the design this replaced
-- only ever looked at a lookback window, so a frame that fell out of it was
-- never reconsidered and froze compaction behind it permanently. The partial
-- index is what makes an unbounded sweep cheap.
--
-- `frame_flags` records this frame's place in the gap structure (NO_DATA,
-- AFTER_GAP, BEFORE_GAP). It lives here rather than on every node's row
-- because gap structure is a property of the frame sequence alone: when a gap
-- fills, that is two row writes instead of 2 x node_count.
CREATE TABLE IF NOT EXISTS graph_history_status (
    timeline_id    TEXT    NOT NULL,
    graph_id       INTEGER NOT NULL,
    ingest_state   TEXT    NOT NULL,
    attempts       INTEGER NOT NULL DEFAULT 0,
    error_blob_key TEXT,
    frame_flags    INTEGER NOT NULL DEFAULT 0,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (timeline_id, graph_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_graph_history_status_pending
    ON graph_history_status(timeline_id, graph_id)
    WHERE ingest_state != 'Ingested';

-- Kept per-node history samples. All of a node's metrics at one frame are
-- packed into `metric_values`, so row count scales with nodes, not metrics.
-- The PK doubles as the chart read index — one node's series over time.
--
-- `reasons` is an OR'd set of the independent justifications for the row:
-- FIRST (the node's first sample), OVER_THRESHOLD (it moved by at least the
-- threshold against the immediately preceding built frame), ANCHOR (the next
-- built frame keeps a crossing this row makes readable), LATEST (this is the
-- newest built frame, so the row is the node's current value).
--
-- The set matters rather than a single winner: a row is routinely both a
-- crossing and the anchor for the crossing after it, which is what happens
-- every time a diff stack lands. Only OVER_THRESHOLD makes a row a baseline.
--
-- `reasons = 0` is legal only at a barrier frame — one flagged AFTER_GAP or
-- BEFORE_GAP, which holds every node's row so the unknown region across the
-- gap is bounded on both sides. Anywhere else a zero-reason row is garbage
-- awaiting collection by `history compact`.
CREATE TABLE IF NOT EXISTS graph_history_entries (
    timeline_id   TEXT    NOT NULL,
    node_name     TEXT    NOT NULL,
    graph_id      INTEGER NOT NULL,
    timestamp     INTEGER NOT NULL,
    metric_values BLOB    NOT NULL,
    reasons       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (timeline_id, node_name, graph_id)
) WITHOUT ROWID;

-- graph_id sits behind node_name in the primary key, so a bare
-- 'WHERE timeline_id = ? AND graph_id = ?' cannot seek without this. The
-- whole-frame reason updates, the segment collapse delete and `history delete`
-- all run exactly that predicate.
CREATE INDEX IF NOT EXISTS idx_graph_history_entries_graph
    ON graph_history_entries(timeline_id, graph_id);

CREATE INDEX IF NOT EXISTS idx_graph_history_entries_ts
    ON graph_history_entries(timeline_id, timestamp);

CREATE TABLE IF NOT EXISTS configs (
    key         TEXT PRIMARY KEY,
    config_type TEXT NOT NULL,
    blob_inline BLOB,
    blob_id     TEXT,
    base_key    TEXT,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER
);

CREATE INDEX IF NOT EXISTS idx_configs_expires
    ON configs(expires_at)
    WHERE expires_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS uniq_ids (
    id INTEGER PRIMARY KEY AUTOINCREMENT
);
";

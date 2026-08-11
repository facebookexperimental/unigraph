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

-- Per-(timeline, graph_id) ingest checkpoint / status.
--
-- `omission_deferred` marks a frame that was ingested while an earlier frame
-- was still unfilled: its threshold verdict could not be trusted, so every
-- node's row was kept. It is `history compact`'s work list. Indexed because
-- compact reads only the flagged span and the flag is 0 for almost every row.
CREATE TABLE IF NOT EXISTS graph_history_status (
    timeline_id       TEXT    NOT NULL,
    graph_id          INTEGER NOT NULL,
    status            TEXT    NOT NULL,
    attempts          INTEGER NOT NULL DEFAULT 0,
    error_blob_key    TEXT,
    omission_deferred INTEGER NOT NULL DEFAULT 0,
    updated_at        INTEGER NOT NULL,
    PRIMARY KEY (timeline_id, graph_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_graph_history_status_deferred
    ON graph_history_status(timeline_id, graph_id)
    WHERE omission_deferred != 0;

-- Kept per-node history samples. All of a node's metrics at one frame are
-- packed into `metric_values`, so row count scales with nodes, not metrics.
-- The PK doubles as the chart read index (one node's series over time) and
-- the 'last kept value before graph_id X' reverse range scan.
--
-- `deferred` marks a below-threshold row that was only written because an
-- earlier frame was still unfilled. Baseline lookups skip these, so a later
-- sample is never measured against a row compaction is about to delete.
--
-- `anchor` marks a below-threshold row kept on purpose: it is the built frame
-- immediately before a surviving sample, and without it that sample's step
-- reads as the whole drift since the last kept row rather than what its own
-- graph contributed. Baseline lookups skip these too — an anchor never cleared
-- the threshold, so measuring against one would hide the accumulated drift.
CREATE TABLE IF NOT EXISTS graph_history_entries (
    timeline_id   TEXT    NOT NULL,
    node_name     TEXT    NOT NULL,
    graph_id      INTEGER NOT NULL,
    timestamp     INTEGER NOT NULL,
    metric_values BLOB    NOT NULL,
    deferred      INTEGER NOT NULL DEFAULT 0,
    anchor        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (timeline_id, node_name, graph_id)
) WITHOUT ROWID;

-- graph_id sits behind node_name in the primary key, so a bare
-- 'WHERE timeline_id = ? AND graph_id = ?' cannot seek without this. Both
-- `history delete` and per-frame compaction run exactly that predicate.
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

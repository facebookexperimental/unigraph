// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SQL DDL constants and migrations for the SQLite storage backend.

/// Table names
pub const TABLE_TIMELINE_CONFIGS: &str = "timeline_configs";
pub const TABLE_GRAPHS: &str = "graphs";
pub const TABLE_BLOBS: &str = "blobs";
pub const TABLE_BLOBS_TO_DELETE: &str = "blobs_to_delete";
pub const TABLE_EXTERNAL_ID_MAPPINGS: &str = "external_id_mappings";
pub const TABLE_METRIC_HISTORY: &str = "metric_history";
pub const TABLE_CONFIGS: &str = "configs";

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
    PRIMARY KEY (timeline_id, graph_id)
);

CREATE INDEX IF NOT EXISTS idx_graphs_timeline_ts
    ON graphs(timeline_id, timestamp, graph_id);

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

CREATE TABLE IF NOT EXISTS configs (
    key         TEXT PRIMARY KEY,
    config_type TEXT NOT NULL,
    blob_inline BLOB,
    blob_id     TEXT,
    base_key    TEXT,
    created_at  INTEGER NOT NULL
);
";

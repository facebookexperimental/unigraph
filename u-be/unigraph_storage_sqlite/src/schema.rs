// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SQL DDL constants and migrations for the SQLite storage backend.

/// SQL statements to create the storage schema.
///
/// Safe to run multiple times (`IF NOT EXISTS`).
pub const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS timelines (
    timeline_id   TEXT PRIMARY KEY,
    config_json   TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS frames (
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

CREATE INDEX IF NOT EXISTS idx_frames_timeline_ts
    ON frames(timeline_id, timestamp, graph_id);

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
";

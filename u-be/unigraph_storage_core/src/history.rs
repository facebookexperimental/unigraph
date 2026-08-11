// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Row structs for the plain-row graph metric history tables.
//!
//! These are dumb data carriers for the `graph_history_*` tables — the
//! packing/threshold logic and the orchestration both live in
//! `unigraph_db::graph_history`.

use crate::traits::GraphIDBounds;
use crate::types::GraphID;
use crate::types::Timestamp;
use crate::types::TimestampBounds;

/// Which history rows to read.
///
/// Both filters apply. Reads are user-facing (`history show` takes dates) but
/// every ordering and adjacency question in this subsystem is a `graph_id`
/// question, so callers that care about correctness — `compact` above all —
/// bound by `graph_ids` and leave `timestamps` open.
#[derive(Debug, Clone, Default)]
pub struct HistoryRange {
    pub timestamps: TimestampBounds,
    pub graph_ids: GraphIDBounds,
}

impl HistoryRange {
    /// Every row, unfiltered.
    pub fn unbounded() -> Self {
        Self::default()
    }
}

/// One kept history sample: all of a node's metrics at one frame, packed
/// into a single binary blob (see `unigraph_db::graph_history::pack`).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntryRow {
    pub node_name: String,
    pub graph_id: GraphID,
    pub timestamp: Timestamp,
    /// Sorted `[(metric_id: u32 LE, value: f64 LE)]` pairs.
    pub values: Vec<u8>,
    /// The row is below the threshold and was only written because an earlier
    /// frame was still unfilled — compaction is expected to delete it.
    ///
    /// Such a row must never be used as the baseline a later sample is
    /// measured against: it is about to disappear, and measuring against it
    /// hides drift that accumulated since the last *surviving* sample. Reads
    /// that resolve a baseline therefore skip these rows entirely.
    pub deferred: bool,
    /// The row exists only to make the *next* sample's frame-over-frame delta
    /// readable — it is the built frame immediately before a surviving sample,
    /// which the threshold would otherwise have folded away.
    ///
    /// Like `deferred`, an anchor is never a baseline: it sits below the
    /// threshold by construction, so measuring against it would hide the drift
    /// accumulated since the last surviving sample and permanently omit a
    /// sample that deserved a row. Unlike `deferred`, it is not provisional —
    /// compaction keeps it for as long as the sample it explains survives.
    pub anchor: bool,
}

/// One stored sample of a node's series, as read back from the entries table.
#[derive(Debug, Clone, PartialEq)]
pub struct HistorySampleRow {
    pub graph_id: GraphID,
    pub timestamp: Timestamp,
    /// Sorted `[(metric_id: u32 LE, value: f64 LE)]` pairs.
    pub values: Vec<u8>,
    /// See [`HistoryEntryRow::anchor`].
    pub anchor: bool,
}

/// One node's stored sample at a single frame.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryNodeSample {
    pub node_name: String,
    /// Sorted `[(metric_id: u32 LE, value: f64 LE)]` pairs.
    pub values: Vec<u8>,
    /// See [`HistoryEntryRow::anchor`].
    pub anchor: bool,
}

/// Per-`(timeline, graph_id)` ingest checkpoint.
///
/// `status` is the string form of `unigraph_db::graph_history::HistoryStatus`.
/// It is kept as a `String` here so the storage layer stays independent of
/// the history logic that defines the enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStatusRow {
    pub graph_id: GraphID,
    pub status: String,
    pub attempts: i64,
    /// Key into blob storage holding the serialized error payload, if the
    /// last attempt failed.
    pub error_blob_key: Option<String>,
    /// The frame was ingested while an earlier frame was still unfilled, so
    /// its threshold decision was deferred and every node's row was kept
    /// unconditionally. `history compact` clears this once the gap closes.
    pub omission_deferred: bool,
}

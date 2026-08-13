// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Row structs for the plain-row graph metric history tables.
//!
//! Dumb data carriers for the `graph_history_*` tables — the packing, the
//! threshold rule and the orchestration all live in
//! `unigraph_db::graph_history`. What the columns *are* lives in [`columns`],
//! next door: a column whose legal values are a fixed set is that set here too,
//! so the trait the backends implement never traffics in bare `u32`s and
//! `String`s that every read site has to re-interpret.

pub mod columns;

pub use columns::FrameFlags;
pub use columns::IngestState;
pub use columns::Reasons;

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

/// A stretch of `graph_id` space with **exclusive** bounds.
///
/// Compaction works between barrier frames, and a barrier's own rows are held
/// by its frame flags whatever their reasons say. Excluding the endpoints is
/// what lets the collapse delete be a single-table range statement with nothing
/// to join against and no node list to send.
///
/// `None` on either side means the timeline's own end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExclusiveGraphIDRange {
    pub after: Option<GraphID>,
    pub before: Option<GraphID>,
}

/// One stored history sample: all of a node's metrics at one frame, packed into
/// a single binary blob (see `unigraph_db::graph_history::pack`).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntryRow {
    pub node_name: String,
    pub graph_id: GraphID,
    pub timestamp: Timestamp,
    /// Sorted `[(metric_id: u32 LE, value: f64 LE)]` pairs.
    pub values: Vec<u8>,
    /// Why this row exists.
    ///
    /// Empty is legal only at a barrier frame, where the row is held by
    /// [`HistoryStatusRow::frame_flags`] instead. Anywhere else a reasonless
    /// row is garbage awaiting collection by `history compact`.
    pub reasons: Reasons,
}

/// One stored sample of a node's series, as read back from the entries table.
#[derive(Debug, Clone, PartialEq)]
pub struct HistorySampleRow {
    pub graph_id: GraphID,
    pub timestamp: Timestamp,
    /// Sorted `[(metric_id: u32 LE, value: f64 LE)]` pairs.
    pub values: Vec<u8>,
    /// See [`HistoryEntryRow::reasons`].
    pub reasons: Reasons,
}

/// One node's stored sample at a single frame.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryNodeSample {
    pub node_name: String,
    /// Sorted `[(metric_id: u32 LE, value: f64 LE)]` pairs.
    pub values: Vec<u8>,
    /// See [`HistoryEntryRow::reasons`].
    pub reasons: Reasons,
}

/// Per-`(timeline, graph_id)` ingest checkpoint.
///
/// Also the work list: every frame whose `ingest_state` is not `Ingested` stays
/// on it, with no time bound. That is what makes an ingest outage a delay
/// rather than a permanent hole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStatusRow {
    pub graph_id: GraphID,
    pub ingest_state: IngestState,
    /// Failed attempts to read this frame. Only meaningful while
    /// `ingest_state` is `Failed`.
    pub attempts: i64,
    /// Key into blob storage holding the serialized error payload, if the last
    /// attempt failed.
    pub error_blob_key: Option<String>,
    /// This frame's place in the gap structure.
    ///
    /// Per frame rather than per row on purpose: when a gap fills, this is two
    /// row writes instead of `2 x node_count`.
    pub frame_flags: FrameFlags,
}

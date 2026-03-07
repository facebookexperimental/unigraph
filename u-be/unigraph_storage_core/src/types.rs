// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Core types for the Unigraph storage layer.
//!
//! Defines identifiers (timelines, graphs), keys, frame types, timeline
//! configuration, and the inline-vs-external blob threshold.

use std::fmt;
use std::str::FromStr;

/// Unique identifier for a timeline — a named, ordered sequence of graphs.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct TimelineID(pub String);

/// Unique identifier for a graph within a timeline.
///
/// Sequential integer assigned during ingestion. Sorts naturally for
/// correct frame ordering when multiple frames share the same timestamp.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct GraphID(pub i64);

/// An identifier from an external source system (e.g. git commit hash, hg revision).
///
/// Not to be confused with [`GraphID`] which is the internal sequential integer ID.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ExternalID(pub String);

/// Namespace for external ID mappings.
///
/// Multiple timelines can share a namespace when they derive from the same
/// source (e.g. same git repo, different graph builders). A namespace like
/// `"my-repo/git"` groups all mappings for one source system.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ExternalIDNamespace(pub String);

/// UTC timestamp — always stored and transmitted in UTC.
pub type Timestamp = unigraph_timestamp::Timestamp;

/// Identifies a specific graph within a specific timeline.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct GraphKey {
    pub timeline_id: TimelineID,
    pub graph_id: GraphID,
}

/// Identifies a specific graph within a timeline at a specific point in time.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct GraphTimeKey {
    pub timeline_id: TimelineID,
    pub timestamp: Timestamp,
    pub graph_id: GraphID,
}

impl GraphTimeKey {
    /// Extract the [`GraphKey`] (without timestamp) from this key.
    pub fn graph_key(&self) -> GraphKey {
        GraphKey {
            timeline_id: self.timeline_id.clone(),
            graph_id: self.graph_id,
        }
    }
}

/// The type of a frame in a timeline.
///
/// This is a simple enum with no associated data. Metadata like the delta
/// base reference lives on [`super::frame::FrameRow`] as a separate field.
///
/// Stored values match enum variant names exactly: `"Empty"`, `"Full"`,
/// `"Delta"`, `"Error"`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FrameType {
    /// Placeholder frame with no data.
    Empty,
    /// Full graph snapshot.
    Full,
    /// Delta-compressed graph derived from a base graph.
    Delta,
    /// Failed graph computation — error details in manifest + blobs.
    Error,
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameType::Empty => write!(f, "Empty"),
            FrameType::Full => write!(f, "Full"),
            FrameType::Delta => write!(f, "Delta"),
            FrameType::Error => write!(f, "Error"),
        }
    }
}

impl FromStr for FrameType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Empty" => Ok(FrameType::Empty),
            "Full" => Ok(FrameType::Full),
            "Delta" => Ok(FrameType::Delta),
            "Error" => Ok(FrameType::Error),
            other => Err(anyhow::anyhow!("Unknown FrameType: {}", other)),
        }
    }
}

/// Sort order for frame queries.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum Order {
    #[default]
    Asc,
    Desc,
}

/// Optional lower/upper timestamp bounds (both inclusive).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimestampBounds {
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
}

/// Structured query for selecting frames from a timeline.
///
/// All filter fields are optional — omitted fields impose no constraint.
/// The storage backend compiles this into a single SQL query.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct FrameQuery {
    /// Which timeline to query.
    pub timeline_id: TimelineID,
    /// Maximum number of rows to return (SQL LIMIT).
    pub limit: Option<i64>,
    /// Only return frames of these types.
    pub frame_types: Option<Vec<FrameType>>,
    /// Sort order by (timestamp, graph_id). Default: Asc.
    pub order: Option<Order>,
    /// Inclusive timestamp bounds.
    pub timestamp_bounds: Option<TimestampBounds>,
    /// Inclusive graph_id bounds: (lower, upper). Either can be None.
    pub graph_id_bounds: Option<(Option<GraphID>, Option<GraphID>)>,
    /// Only return frames with these specific graph_ids (SQL IN).
    pub graph_ids: Option<Vec<GraphID>>,
    /// If true, fetch full row data (manifest + inline blobs).
    /// If false/None, return metadata only (data = None).
    pub with_data: Option<bool>,
    /// Select the frame immediately before this (timestamp, graph_id) point.
    /// Compiles to: `WHERE (timestamp < X OR (timestamp = X AND graph_id < Y))`
    /// with `ORDER BY timestamp DESC, graph_id DESC LIMIT 1`.
    pub before: Option<(Timestamp, GraphID)>,
}

/// A timestamped error message from a failed graph computation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimestampedError {
    pub timestamp: Timestamp,
    pub message: String,
}

/// Schema that governs how frames in a timeline relate to each other.
///
/// Currently only [`AdjacentDeltas`](AdjacentDeltasConfig) is supported:
/// deltas must reference the immediately preceding frame as their base.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TimelineSchema {
    /// Linear history where deltas derive from the immediately preceding graph.
    AdjacentDeltas(AdjacentDeltasConfig),
}

impl fmt::Display for TimelineSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimelineSchema::AdjacentDeltas(_) => write!(f, "AdjacentDeltas"),
        }
    }
}

/// Configuration for the [`TimelineSchema::AdjacentDeltas`] schema.
///
/// Empty for now — fields will be added as the schema evolves
/// (e.g. max delta chain length, compaction policy).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdjacentDeltasConfig {}

/// Controls where graph blobs are stored for a timeline.
///
/// - `Inline`: blobs under the size threshold are compressed and stored
///   directly in the frames table row. This is the default.
/// - `External`: blobs are always stored in the external blob storage,
///   regardless of size.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum BlobStorageMode {
    /// Store blobs inline when total size ≤ threshold (default behavior).
    #[default]
    Inline,
    /// Always store blobs in external blob storage.
    External,
}

/// Timeline configuration stored as a JSON blob in the `timelines` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineConfig {
    pub schema: TimelineSchema,
    /// Optional namespace for external ID mappings. When set, this timeline's
    /// GraphIDs can be resolved back to ExternalIDs via the mapping table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id_namespace: Option<ExternalIDNamespace>,
    /// Controls whether blobs are stored inline or always externally.
    /// Defaults to `Inline` (blobs under 50 KB are stored in the frames table).
    #[serde(default)]
    pub blob_storage: BlobStorageMode,
}

impl TimelineConfig {
    /// Get the inline blob threshold in bytes for this timeline.
    ///
    /// - `Inline` → the default 50 KB threshold
    /// - `External` → 0 (all blobs go to external storage)
    pub fn inline_blob_threshold(&self) -> usize {
        match self.blob_storage {
            BlobStorageMode::Inline => DEFAULT_INLINE_BLOB_THRESHOLD_BYTES,
            BlobStorageMode::External => 0,
        }
    }
}

/// Default total blob size threshold (in bytes) below which blobs are stored
/// inline in the frames table rather than in external blob storage.
pub const DEFAULT_INLINE_BLOB_THRESHOLD_BYTES: usize = 50_000; // 50 KB

/// Total blob size threshold (in bytes) below which blobs are stored inline
/// in the frames table rather than in external blob storage.
#[deprecated(note = "Use TimelineConfig::inline_blob_threshold() instead")]
pub const INLINE_BLOB_THRESHOLD_BYTES: usize = DEFAULT_INLINE_BLOB_THRESHOLD_BYTES;

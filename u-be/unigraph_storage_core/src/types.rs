// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Core types for the Unigraph storage layer.
//!
//! Identifier types (`TimelineID`, `GraphID`, `GraphKey`, `GraphTimeKey`,
//! `Timestamp`) are re-exported from `unigraph_core::identifiers`.
//! This module defines storage-specific types: frame types, timeline
//! configuration, and the inline-vs-external blob threshold.

use std::fmt;
use std::str::FromStr;

pub use unigraph_core::GraphID;
pub use unigraph_core::GraphKey;
pub use unigraph_core::GraphTimeKey;
pub use unigraph_core::TimelineID;
pub use unigraph_core::Timestamp;

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
    serde::Deserialize,
    typegen::TypeGen
)]
pub struct ExternalIDNamespace(pub String);

pub use unigraph_core::GraphKeyOrTimelineID;

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
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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
    /// If true, fetch the frame's manifest but not its inline blobs.
    ///
    /// Cheap next to [`with_data`](Self::with_data): a manifest is a short JSON
    /// string, where `inline_blobs` beside it can hold the frame's whole
    /// compressed graph. Use this to read many frames' manifests at once —
    /// e.g. to find which external blobs a range of frames references.
    /// [`FrameRow::blobs_are_inline`](crate::frame::FrameRow::blobs_are_inline)
    /// is still answered, so "does this frame own external blobs?" needs no
    /// payload read. Implied by `with_data`.
    pub with_manifest: Option<bool>,
    /// If true, fetch full row data (manifest + inline blobs).
    /// If false/None, the payload columns are left unpopulated unless
    /// [`with_manifest`](Self::with_manifest) asked for the manifest.
    pub with_data: Option<bool>,
    /// Select the frame immediately before this (timestamp, graph_id) point.
    /// Compiles to: `WHERE (timestamp < X OR (timestamp = X AND graph_id < Y))`
    /// with `ORDER BY timestamp DESC, graph_id DESC LIMIT 1`.
    pub before: Option<(Timestamp, GraphID)>,
    /// Only return frames whose `expires_at` column is non-NULL and <= this value.
    /// Used for TTL cleanup: pass `Timestamp::now()` to find expired frames.
    pub expires_before: Option<Timestamp>,
}

/// A timestamped error message from a failed graph computation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimestampedError {
    pub timestamp: Timestamp,
    pub message: String,
}

/// Schema that governs how frames in a timeline relate to each other.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, typegen::TypeGen)]
pub enum TimelineSchema {
    /// Simple schema where deltas can reference any graph as a base.
    ///
    /// No ordering constraints, no adjacent-base requirements. Deltas are
    /// created explicitly via `store_as_delta_from` and can reference graphs
    /// in other timelines. Compaction is not supported.
    FullOrDelta(FullOrDeltaConfig),

    /// Linear history where deltas derive from the immediately preceding graph.
    ///
    /// Enforces monotonic `(timestamp, graph_id)` ordering and adjacent delta
    /// base references. Supports compaction (replacing Full frames with Deltas).
    /// Optimized for high-throughput timelines with iterative range-query fetch.
    AdjacentDeltas(AdjacentDeltasConfig),
}

impl fmt::Display for TimelineSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimelineSchema::AdjacentDeltas(_) => write!(f, "AdjacentDeltas"),
            TimelineSchema::FullOrDelta(_) => write!(f, "FullOrDelta"),
        }
    }
}

/// Configuration for the [`TimelineSchema::AdjacentDeltas`] schema.
///
/// Empty for now — fields will be added as the schema evolves
/// (e.g. max delta chain length, compaction policy).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, typegen::TypeGen)]
pub struct AdjacentDeltasConfig {}

/// Configuration for the [`TimelineSchema::FullOrDelta`] schema.
///
/// Empty for now — the schema has no configuration knobs yet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, typegen::TypeGen)]
pub struct FullOrDeltaConfig {}

/// Controls where graph blobs are stored for a timeline.
///
/// - `Inline`: blobs under the size threshold are compressed and stored
///   directly in the frames table row. This is the default.
/// - `External`: blobs are always stored in the external blob storage,
///   regardless of size.
#[derive(
    Debug,
    Clone,
    Default,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen
)]
pub enum BlobStorageMode {
    /// Store blobs inline when total size ≤ threshold (default behavior).
    #[default]
    Inline,
    /// Always store blobs in external blob storage.
    External,
}

/// Timeline configuration stored as a JSON blob in the `timelines` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, typegen::TypeGen)]
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
    /// When `Some(true)`, per-node metric history is stored alongside each
    /// graph frame in the same transaction. History blobs are partitioned by
    /// ISO week for bounded blob sizes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_metric_history: Option<bool>,
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

/// A config row on its way into the configs table.
///
/// Type-erased on purpose: the storage layer needs the string key and the type
/// tag, not the key's Rust type. That lets one batch carry `tvc_` and `gqc_`
/// rows together, and keeps the backends to a single write method instead of
/// one per config kind.
///
/// Exactly one of `blob_inline` / `blob_id` is set — inline for small configs,
/// an external blob path for large ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWrite {
    /// Content-addressed key, e.g. `tvc_1f4a09c6b73e5d82`.
    pub key: String,
    /// Config kind, from `ConfigKeyLike::PREFIX`.
    pub config_type: String,
    pub blob_inline: Option<Vec<u8>>,
    pub blob_id: Option<String>,
    pub expires_at: Option<Timestamp>,
}

/// Default total blob size threshold (in bytes) below which blobs are stored
/// inline in the frames table rather than in external blob storage.
pub const DEFAULT_INLINE_BLOB_THRESHOLD_BYTES: usize = 50_000; // 50 KB

/// Total blob size threshold (in bytes) below which blobs are stored inline
/// in the frames table rather than in external blob storage.
#[deprecated(note = "Use TimelineConfig::inline_blob_threshold() instead")]
pub const INLINE_BLOB_THRESHOLD_BYTES: usize = DEFAULT_INLINE_BLOB_THRESHOLD_BYTES;

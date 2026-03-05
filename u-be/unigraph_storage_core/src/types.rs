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
/// Stored as a `String` to support commit hashes and other non-numeric IDs.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct GraphID(pub String);

/// UTC timestamp — always stored and transmitted in UTC.
pub type Timestamp = chrono::DateTime<chrono::Utc>;

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
            graph_id: self.graph_id.clone(),
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

/// Timeline configuration stored as a JSON blob in the `timelines` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineConfig {
    pub schema: TimelineSchema,
}

/// Total blob size threshold (in bytes) below which blobs are stored inline
/// in the frames table rather than in external blob storage.
pub const INLINE_BLOB_THRESHOLD_BYTES: usize = 50_000; // 50 KB

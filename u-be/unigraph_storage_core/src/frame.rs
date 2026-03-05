// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Frame types for the storage layer.
//!
//! A [`Frame`] is a point in a timeline (timestamp + graph ID). A [`FrameRow`]
//! is a full database row including frame type, optional delta base, and
//! optional payload data.

use crate::types::FrameType;
use crate::types::GraphID;
use crate::types::GraphKey;
use crate::types::TimelineID;
use crate::types::Timestamp;

/// A point in a timeline: timestamp + graph ID.
#[derive(Debug, Clone)]
pub struct Frame {
    pub timestamp: Timestamp,
    pub graph_id: GraphID,
}

/// A frame row from the database. Can be fetched with or without data.
#[derive(Debug, Clone)]
pub struct FrameRow {
    /// The frame's position in the timeline.
    pub frame: Frame,
    /// Which timeline this frame belongs to.
    pub timeline_id: TimelineID,
    /// The type of this frame (Empty, Full, Delta, Error).
    pub frame_type: FrameType,
    /// For Delta frames: the base graph this delta is derived from.
    /// Stored in a separate column, always populated for Delta, `None` otherwise.
    pub base: Option<GraphKey>,
    /// The actual frame payload. `None` when fetched in "metadata only" mode
    /// (e.g. listing timeline contents, checking for gaps/errors).
    /// `Some` when fetched with data (e.g. reading graphs for reconstruction).
    pub data: Option<FrameData>,
}

/// The payload data for a frame.
///
/// All data-carrying frame types (Full, Delta, Error) use the same shape:
/// a JSON manifest string plus optional inline blobs.
#[derive(Debug, Clone)]
pub struct FrameData {
    /// JSON-serialized manifest. The concrete type depends on `frame_type`:
    /// - Full → `ArrayGraphSerializableManifest`
    /// - Delta → `DeltaManifest`
    /// - Error → `ErrorManifest`
    ///
    /// Stored as raw JSON so the storage trait doesn't need to know the
    /// manifest type.
    pub manifest_json: String,
    /// ZSTD-compressed serialized `BTreeMap<BlobID, Vec<u8>>`.
    /// `Some` when blobs are stored inline (small data).
    /// `None` when blobs are in external blob storage.
    pub inline_blobs: Option<Vec<u8>>,
}

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
///
/// Ordered by `(timestamp, graph_id)`, matching the SQL `ORDER BY` convention.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frame {
    pub timestamp: Timestamp,
    pub graph_id: GraphID,
}

/// A frame row from the database.
///
/// The payload columns (`manifest_json`, `inline_blobs`) are populated
/// according to what the [`FrameQuery`](crate::types::FrameQuery) asked for:
///
/// | query                 | `manifest_json` | `inline_blobs` | `blobs_are_inline` |
/// |-----------------------|-----------------|----------------|--------------------|
/// | neither flag          | `None`          | `None`         | `None`             |
/// | `with_manifest`       | the manifest    | `None`         | `Some(_)`          |
/// | `with_data`           | the manifest    | the blobs      | `Some(_)`          |
///
/// The middle row is the point of the split: the manifest is a short JSON
/// string, while `inline_blobs` beside it can hold the frame's entire
/// compressed graph. Reading a batch of manifests is cheap; reading a batch of
/// payloads is hundreds of megabytes.
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
    /// JSON-serialized manifest. The concrete type depends on `frame_type`:
    /// - Full → `ArrayGraphSerializableManifest`
    /// - Delta → `DeltaManifest`
    /// - Error → `ErrorManifest`
    /// - Empty → no manifest at all
    ///
    /// Stored as raw JSON so the storage trait doesn't need to know the
    /// manifest type. `None` on an `Empty` frame, and on any read that asked
    /// for neither `with_manifest` nor `with_data`.
    pub manifest_json: Option<String>,
    /// ZSTD-compressed serialized `BTreeMap<BlobID, Vec<u8>>`.
    ///
    /// `None` covers three different situations — the frame's blobs are
    /// external, the frame is `Empty`, or the read didn't ask for the payload.
    /// Consult [`blobs_are_inline`](Self::blobs_are_inline) to tell them apart;
    /// testing this field alone is only sound after a `with_data` read.
    pub inline_blobs: Option<Vec<u8>>,
    /// Whether the frame's blobs live in the `inline_blobs` column rather than
    /// in external storage.
    ///
    /// `None` on a metadata-only read, which does not look at the payload
    /// columns at all. Any read that does look answers this, including
    /// `with_manifest` — it costs an `IS NOT NULL`, not the bytes, which is
    /// what lets a bulk reader decide whether a frame owns external blobs
    /// without ever loading a payload.
    pub blobs_are_inline: Option<bool>,
    /// Optional expiration timestamp. `None` means the frame never expires.
    /// When set, the frame is eligible for cleanup after this time.
    pub expires_after: Option<Timestamp>,
}

/// Format a list of [`FrameRow`]s as an ASCII table.
pub fn format_frames_table(frames: &[FrameRow]) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:<20} {:<24} {:<10} {:<10} {:<24}",
        "graph_id", "timestamp", "type", "base", "expires_at"
    ));
    lines.push("-".repeat(94));

    for frame in frames {
        let base_str = match &frame.base {
            Some(key) => format!("{}:{}", key.timeline_id.0, key.graph_id.0),
            None => "-".to_string(),
        };
        let expires_str = match &frame.expires_after {
            Some(ts) => ts.to_comparable_rfc3339_str(),
            None => "-".to_string(),
        };
        lines.push(format!(
            "{:<20} {:<24} {:<10} {:<10} {:<24}",
            frame.frame.graph_id.0,
            frame.frame.timestamp.to_comparable_rfc3339_str(),
            frame.frame_type,
            base_str,
            expires_str,
        ));
    }

    lines
        .iter()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

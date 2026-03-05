// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Storage trait definitions for the Unigraph storage layer.
//!
//! [`UnigraphGraphStorage`] handles frame metadata and inline data.
//! [`UnigraphBlobStorage`] handles external blob storage for large payloads.

use anyhow::Result;

use crate::frame::FrameRow;
use crate::types::FrameType;
use crate::types::GraphKey;
use crate::types::GraphTimeKey;
use crate::types::TimelineConfig;
use crate::types::TimelineID;
use crate::types::Timestamp;

/// Graph storage trait — manages timelines, frames, and inline blob data.
///
/// Implementations store frame metadata (type, timestamp, base reference)
/// alongside optional manifest JSON and inline blob data. The `with_data`
/// parameter on fetch methods controls whether the potentially large
/// manifest + blob columns are read.
pub trait UnigraphGraphStorage: Send + Sync {
    /// Create a new timeline with the given configuration.
    fn create_timeline(&self, timeline_id: &TimelineID, config: &TimelineConfig) -> Result<()>;

    /// Get the configuration for an existing timeline.
    /// Returns `None` if the timeline does not exist.
    fn get_timeline_config(&self, timeline_id: &TimelineID) -> Result<Option<TimelineConfig>>;

    /// List all timeline IDs.
    fn list_timelines(&self) -> Result<Vec<TimelineID>>;

    /// Store a frame with data (Full, Delta, or Error).
    ///
    /// - `key`: timeline + timestamp + graph ID
    /// - `frame_type`: the type of frame (should not be `Empty` — use [`store_frame_empty`] for that)
    /// - `base`: for Delta frames, the base graph this delta derives from; `None` otherwise
    /// - `manifest_json`: JSON-serialized manifest
    /// - `inline_blobs`: optional ZSTD-compressed blob map (when blobs are small enough to inline)
    fn store_frame(
        &self,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
    ) -> Result<()>;

    /// Store an empty frame (placeholder with no data).
    fn store_frame_empty(&self, key: &GraphTimeKey) -> Result<()>;

    /// Fetch a single frame by graph key.
    ///
    /// - `with_data: false` → `FrameRow { data: None }` — fast metadata-only read
    /// - `with_data: true` → `FrameRow { data: Some(FrameData { .. }) }` — includes manifest + blobs
    ///
    /// Returns `None` if the frame does not exist.
    fn get_frame(&self, key: &GraphKey, with_data: bool) -> Result<Option<FrameRow>>;

    /// List all frames in a timeline, ordered by (timestamp, graph_id).
    /// Always returns metadata only (data is `None`).
    fn list_frames(&self, timeline_id: &TimelineID) -> Result<Vec<FrameRow>>;

    /// List frames in a timeline within a time range, ordered by (timestamp, graph_id).
    /// Always returns metadata only (data is `None`).
    fn list_frames_range(
        &self,
        timeline_id: &TimelineID,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Vec<FrameRow>>;

    /// Get the frame immediately preceding the given key in the timeline
    /// (by timestamp, then graph_id).
    /// Always returns metadata only (data is `None`).
    fn get_preceding_frame(&self, key: &GraphTimeKey) -> Result<Option<FrameRow>>;

    /// Register blob keys for deferred cleanup.
    ///
    /// Used during store operations: if the transaction fails after blobs
    /// have been uploaded to external storage, these keys can be cleaned up later.
    fn register_blobs_for_cleanup(&self, blob_keys: &[String]) -> Result<()>;

    /// Unregister blob keys from the cleanup list (transaction succeeded).
    fn unregister_blobs_for_cleanup(&self, blob_keys: &[String]) -> Result<()>;

    /// Get all blob keys that are pending cleanup.
    fn get_blobs_pending_cleanup(&self) -> Result<Vec<String>>;
}

/// External blob storage trait — manages large blobs outside the frames table.
///
/// Used when the total blob size exceeds [`INLINE_BLOB_THRESHOLD_BYTES`](crate::types::INLINE_BLOB_THRESHOLD_BYTES).
pub trait UnigraphBlobStorage: Send + Sync {
    /// Store a blob by key.
    fn put_blob(&self, key: &str, data: &[u8]) -> Result<()>;

    /// Retrieve a blob by key. Returns `None` if not found.
    fn get_blob(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Delete a blob by key.
    fn delete_blob(&self, key: &str) -> Result<()>;

    /// Check if a blob exists.
    fn has_blob(&self, key: &str) -> Result<bool>;

    /// List all blob keys matching a prefix.
    fn list_blobs(&self, prefix: &str) -> Result<Vec<String>>;
}

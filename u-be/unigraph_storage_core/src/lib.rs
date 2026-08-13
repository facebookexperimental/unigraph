// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Core storage layer for Unigraph graph lineage.
//!
//! Provides types and traits for persisting ordered sequences of graphs
//! (timelines) with support for full snapshots, delta-compressed frames,
//! and error recording.
//!
//! ## Crate layout
//!
//! - [`types`] — identifiers, keys, frame types, timeline configuration
//! - [`frame`] — frame and frame-row structures
//! - [`history`] — row structs for the plain-row graph metric history tables
//! - [`traits`] — storage trait definitions ([`UnigraphGraphStorage`], [`UnigraphGraphConnection`], [`UnigraphBlobStorage`])

pub mod config_key {
    //! Re-exports from `unigraph_core::config_key`.
    pub use unigraph_core::config_key::*;
}
pub mod frame;
pub mod history;
pub mod traits;
pub mod types;

pub use config_key::ConfigKeyLike;
pub use config_key::ConfigRow;
pub use config_key::GraphQueryConfigKey;
pub use config_key::TraversalConfigKey;
pub use frame::Frame;
pub use frame::FrameData;
pub use frame::FrameRow;
pub use frame::format_frames_table;
pub use history::ExclusiveGraphIDRange;
pub use history::FrameFlags;
pub use history::HistoryEntryRow;
pub use history::HistoryNodeSample;
pub use history::HistoryRange;
pub use history::HistorySampleRow;
pub use history::HistoryStatusRow;
pub use history::IngestState;
pub use history::Reasons;
pub use traits::GraphIDBounds;
pub use traits::UnigraphBlobStorage;
pub use traits::UnigraphGraphConnection;
pub use traits::UnigraphGraphStorage;
pub use types::AdjacentDeltasConfig;
pub use types::BlobStorageMode;
pub use types::DEFAULT_INLINE_BLOB_THRESHOLD_BYTES;
pub use types::ExternalID;
pub use types::ExternalIDNamespace;
pub use types::FrameQuery;
pub use types::FrameType;
pub use types::FullOrDeltaConfig;
pub use types::GraphID;
pub use types::GraphKey;
pub use types::GraphKeyOrTimelineID;
pub use types::GraphTimeKey;
#[allow(deprecated)]
pub use types::INLINE_BLOB_THRESHOLD_BYTES;
pub use types::Order;
pub use types::TimelineConfig;
pub use types::TimelineID;
pub use types::TimelineSchema;
pub use types::Timestamp;
pub use types::TimestampBounds;
pub use types::TimestampedError;

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Core storage layer for Unigraph graph lineage.
//!
//! Provides types, traits, and a compositor for persisting ordered sequences
//! of graphs (timelines) with support for full snapshots, delta-compressed
//! frames, and error recording.
//!
//! ## Crate layout
//!
//! - [`types`] — identifiers, keys, frame types, timeline configuration
//! - [`frame`] — frame and frame-row structures
//! - [`traits`] — storage trait definitions ([`UnigraphGraphStorage`], [`UnigraphBlobStorage`])
//! - [`storage`] — [`UnigraphStorage`] compositor (store/fetch logic)

pub mod frame;
pub mod storage;
pub mod traits;
pub mod types;

pub use frame::Frame;
pub use frame::FrameData;
pub use frame::FrameRow;
pub use storage::UnigraphStorage;
pub use traits::UnigraphBlobStorage;
pub use traits::UnigraphGraphStorage;
pub use types::AdjacentDeltasConfig;
pub use types::FrameType;
pub use types::GraphID;
pub use types::GraphKey;
pub use types::GraphTimeKey;
pub use types::INLINE_BLOB_THRESHOLD_BYTES;
pub use types::TimelineConfig;
pub use types::TimelineID;
pub use types::TimelineSchema;
pub use types::Timestamp;
pub use types::TimestampedError;

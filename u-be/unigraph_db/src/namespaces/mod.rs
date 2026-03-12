// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Namespaced API surface for [`UnigraphDb`](crate::UnigraphDb).
//!
//! Each module defines a lightweight handle struct that provides a focused
//! subset of the database API. Handles hold `Arc<UnigraphStorage>` and
//! are cheaply cloneable.

mod blob_storage;
mod external_ids;
mod frames;
mod graph;
mod metric_history;
mod timelines;

pub use blob_storage::BlobStorageOps;
pub use external_ids::ExternalIds;
pub use frames::Frames;
pub use graph::Graph;
pub use metric_history::MetricHistory;
pub use timelines::Timelines;

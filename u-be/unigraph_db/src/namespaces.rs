// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Namespaced API surface for [`UnigraphDb`](crate::UnigraphDb).
//!
//! Each module defines a lightweight handle struct that provides a focused
//! subset of the database API. Handles hold a shared [`UnigraphDbContext`](crate::context::UnigraphDbContext)
//! and are cheaply cloneable.

mod adjacent_deltas_ops;
mod blob_storage;
mod configs;
mod external_ids;
mod frames;
mod graph;
mod graph_history;
mod metric_history;
mod timelines;
mod utility;

pub use adjacent_deltas_ops::AdjacentDeltasOps;
pub use blob_storage::BlobStorageOps;
pub use configs::Configs;
pub use external_ids::ExternalIds;
pub use frames::Frames;
pub use graph::Graph;
pub use graph_history::GraphHistory;
pub use graph_history::HistoryCompactOptions;
pub use graph_history::HistoryCompactReport;
pub use graph_history::HistoryDeleteReport;
pub use graph_history::HistoryIngestOptions;
pub use graph_history::HistoryIngestReport;
pub use graph_history::HistorySeriesRow;
pub use metric_history::MetricHistory;
pub use timelines::Timelines;
pub use utility::CleanupResult;
pub use utility::Utility;

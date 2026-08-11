// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Pure transforms for the plain-row graph metric history (`unigraph history`).
//!
//! This module holds only DB-agnostic logic: value packing, threshold
//! filtering, compaction decisions, and the status/error payload types. The
//! orchestration that uses them — transactions, locking, retries, blob
//! cleanup — lives in [`crate::namespaces::GraphHistory`], and the SQL lives
//! in the storage backend.
//!
//! ```text
//! extract_node_metrics        node -> metric name -> f64   (unigraph_metric_history)
//!   -> intern metric names    metric name -> u32           (storage)
//!   -> settle::is_frame_settled  may omitting be trusted?  (here)
//!   -> threshold::keep_row    is this sample worth a row?  (here)
//!   -> pack::encode_values    BTreeMap<u32, f64> -> blob   (here)
//! ```
//!
//! # The asymmetry everything here is built around
//!
//! **Keeping a row is reversible; omitting one is not.** `compact` can drop a
//! row that later proves redundant, but nothing can resurrect a sample that was
//! never written. Because the source timeline fills frames out of order, a
//! threshold verdict taken across an unfilled gap can be invalidated by a frame
//! that arrives later — so [`settle`] gates omission, and anything it can't
//! vouch for is kept and flagged `omission_deferred` for `compact` to revisit.
//!
//! # The other thing omission destroys
//!
//! A surviving sample is an absolute, and the row before it may be hundreds of
//! frames back, so its step reads as all the drift since the last kept row
//! rather than what its own graph contributed. Ingest therefore also keeps the
//! row at the frame immediately before each surviving sample — an *anchor*.
//! Anchors are never baselines and are never judged by the threshold; see
//! [`crate::namespaces::GraphHistory`] for the full rules.
//!
//! Kept separate from [`crate::metric_history`], which is the older
//! blob-per-node-per-week subsystem written inside the graph store
//! transaction.

pub mod compact;
pub mod pack;
pub mod settle;
pub mod status;
pub mod threshold;

pub use compact::CompactInput;
pub use compact::CompactPlan;
pub use compact::CompactRow;
pub use compact::compact_series;
pub use pack::decode_values;
pub use pack::encode_values;
pub use settle::is_frame_settled;
pub use status::ErrorPayload;
pub use status::HistoryStatus;
pub use threshold::keep_row;

/// How many times a frame may fail ingestion before it is abandoned.
///
/// Without a cap, a permanently-broken frame would burn a graph fetch on
/// every scheduled run forever.
pub const MAX_ATTEMPTS: u32 = 5;

/// Default age at which an unfilled frame is presumed abandoned.
///
/// Frames are registered in order but filled out of order, so omitting a sample
/// is only safe once every earlier frame has stopped changing. Waiting forever
/// is not an option — a large share of `www-budget`'s frames are Empty
/// placeholders that will never be filled (their source counterpart failed to
/// build), and they would pin the settled frontier permanently.
///
/// 48h is deliberately generous against the source pipeline's observed
/// catch-up latency. Raising it costs storage (more deferred rows waiting on
/// compaction); lowering it risks a late fill silently distorting a series.
pub const DEFAULT_SETTLE_HOURS: usize = 48;

pub const STATUS_PROCESSED: &str = "Processed";
pub const STATUS_OMITTED: &str = "Omitted";
pub const STATUS_ERROR: &str = "Error";
pub const STATUS_EMPTY: &str = "Empty";

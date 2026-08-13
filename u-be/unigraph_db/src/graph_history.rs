// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Pure transforms for the plain-row graph metric history (`unigraph history`).
//!
//! DB-agnostic logic only: value packing, the threshold rule, the reason and
//! gap-flag sets, and compaction decisions. The orchestration that uses them —
//! transactions, locking, retries, blob cleanup — lives in
//! [`crate::namespaces::GraphHistory`], and the SQL lives in the storage
//! backends.
//!
//! ```text
//! extract_node_metrics       node -> metric name -> f64   (unigraph_metric_history)
//!   -> intern metric names   metric name -> u32           (storage)
//!   -> threshold::crosses    did this node move?          (here)
//!   -> reasons::Reasons      why does this row exist?     (here)
//!   -> gaps::FrameFlags      what bounds the unknown?     (here)
//!   -> pack::encode_values   BTreeMap<u32, f64> -> blob   (here)
//! ```
//!
//! # What the subsystem is for
//!
//! A *timeline* is an ordered sequence of graphs, one per landed diff. History
//! answers one question about it: **which diff moved this node?** Recording
//! every node at every frame is unaffordable — `www-budget` produced 24,252
//! frames in six days — so a node's value is recorded only where it moved.
//!
//! # The rule everything follows from
//!
//! ```text
//! crossing at N  <=>  |value(N) - value(N-1)| >= threshold      N-1 = previous BUILT frame
//! ```
//!
//! Measured against the **immediately preceding built frame**, never against
//! the node's last kept row. See [`threshold`] for why that single choice
//! deletes an entire category of hazard, and what it deliberately gives up.
//!
//! # The three things a row can be
//!
//! Not an enum — a set of OR'd [`Reasons`], because a row is routinely more
//! than one of them at once. Plus a fourth case that is not a reason at all:
//! the built frames bounding a gap keep every node's row through their
//! [`FrameFlags`], which is a property of the frame, not the row.
//!
//! Both of those, and [`IngestState`], are defined in
//! [`unigraph_storage_core`] and re-exported here: they are stored columns, so
//! they live with the rows that carry them and the trait the backends
//! implement never sees a bare `u32`. The *rules* that decide when to set them
//! are what lives in this module.
//!
//! ```text
//! row exists     <=>  reasons != 0  OR  the frame is a barrier
//! is a baseline  <=>  reasons contains OVER_THRESHOLD
//! ```
//!
//! # The asymmetry to keep in mind
//!
//! **Keeping a row is reversible; omitting one is not.** `compact` can drop a
//! row that proves redundant, but nothing resurrects a sample never written.
//! Every ambiguous decision in here errs toward keeping.
//!
//! Kept separate from [`crate::metric_history`], which is the older
//! blob-per-node-per-week subsystem written inside the graph store transaction.

pub mod compact;
pub mod gaps;
pub mod pack;
pub mod status;
pub mod threshold;

pub use compact::CompactInput;
pub use compact::CompactPlan;
pub use compact::CompactRow;
pub use compact::compact_series;
pub use gaps::FlagUpdate;
pub use gaps::FrameGap;
pub use gaps::Segment;
pub use gaps::desired_flags;
pub use gaps::frame_has_data;
pub use gaps::only_frame;
pub use gaps::reconcile_flags;
pub use gaps::segments;
pub use pack::decode_values;
pub use pack::encode_values;
pub use status::ErrorPayload;
pub use threshold::Values;
pub use threshold::crosses;
// The stored column types. Re-exported so the whole subsystem can say
// `graph_history::Reasons` without caring which layer defines it.
pub use unigraph_storage_core::FrameFlags;
pub use unigraph_storage_core::IngestState;
pub use unigraph_storage_core::Reasons;

/// How many times a frame may fail ingestion before it is abandoned.
///
/// Without a cap, a permanently-broken frame would burn a graph fetch on every
/// scheduled run forever. A frame past the cap stays a gap, which is the honest
/// description of it — we have no values there — and its neighbours keep
/// boundary rows accordingly.
pub const MAX_ATTEMPTS: u32 = 5;

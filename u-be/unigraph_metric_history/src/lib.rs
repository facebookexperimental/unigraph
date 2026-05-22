// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Metric history for Unigraph — tracks how per-node metrics evolve over time.
//!
//! # What is MetricHistory?
//!
//! MetricHistory is a data structure that tracks how a `BTreeMap<MetricName, f64>`
//! evolves over an ordered sequence of graph frames `(Timestamp, GraphID)`.
//! In Unigraph's context, this tracks how per-node metrics (like JS bundle sizes)
//! change across graph snapshots over time.
//!
//! Instead of storing the full metric map at every frame, it only stores **deltas**
//! — what changed — at frames where metrics actually changed. Frames where nothing
//! changed are skipped entirely (sparse storage).
//!
//! ```text
//! Frame:    1          2          3          4          5          6
//! Value:    {a:100}    {a:100}    {a:200}    {a:200}    {a:200}    {a:300}
//!
//! Stored:   [Δ1]       —          [Δ3]       —          —          [Δ6]
//!           {a:100}               {a:+100}                         {a:+100}
//! ```
//!
//! Only 3 deltas stored instead of 6 full snapshots. The longer a value stays
//! unchanged, the bigger the savings.
//!
//! # Storage architecture
//!
//! History blobs are partitioned by `(TimelineID, NodeName, ISO Week)`. Each
//! blob is a ZSTD-compressed [`FlatHistory`] containing all the delta-encoded
//! frames for one node in one week.
//!
//! ```text
//! metric_history table:
//!   ┌──────────────┬───────────┬──────────┬──────────────────────────┐
//!   │ timeline_id  │ node_name │ week_key │ data (ZSTD blob)         │
//!   ├──────────────┼───────────┼──────────┼──────────────────────────┤
//!   │ "my-app"     │ "Button"  │ 2025-W02 │ <FlatHistory bytes>      │
//!   │ "my-app"     │ "Button"  │ 2025-W03 │ <FlatHistory bytes>      │
//!   │ "my-app"     │ "Header"  │ 2025-W02 │ <FlatHistory bytes>      │
//!   └──────────────┴───────────┴──────────┴──────────────────────────┘
//! ```
//!
//! Weekly partitioning keeps individual blobs small (~7 entries per node per
//! week at daily ingestion frequency). Each partition starts fresh with an
//! absolute first frame — no cross-partition dependencies.
//!
//! # Key edge cases
//!
//! ## Middle insertion
//!
//! Frames can be inserted out of order. When inserting into a sparse range,
//! [`FlatHistory::insert`] materializes anchor points and recomputes all
//! deltas from scratch. See the `flat_history` module for details.
//!
//! ## Absent nodes
//!
//! When a graph is stored that doesn't contain a node that previously existed,
//! an explicit `None` entry is recorded for that node. Without this, the
//! sparse delta chain would incorrectly imply the node's metrics persisted.
//!
//! ## Transactional consistency
//!
//! History is stored in the **same database transaction** as the graph frame.
//! If the graph write fails, history is rolled back too.
//!
//! # Crate layout
//!
//! - [`types`] — `WeekPartition`, `NodeMetricSnapshot`, `Frame`
//! - [`flat_history`] — `FlatHistory` data structure with insert, reconstruct, serialize
//! - [`extract`] — extract per-node metrics from `ArrayGraphSerializable`

pub mod extract;
pub mod flat_history;
pub mod types;

pub use extract::extract_node_metrics;
pub use flat_history::FlatHistory;
pub use types::NodeMetricSnapshot;
pub use types::WeekPartition;

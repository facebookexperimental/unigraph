// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Read per-node metric history — the RPC equivalent of `unigraph history show`.
//!
//! # Wire shape
//!
//! History is long and repetitive: the same handful of metric names and the
//! same frames recur on every sample, for every node. Sending rows of
//! `{node, timestamp, {metric: value}}` would spend most of its bytes on those
//! repeats, so the output is columnar instead — two dictionaries plus samples
//! that index into them.
//!
//! ```text
//! metrics: ["lines", "size"]                     <- names sent once
//! frames:  [{0, "…T00:00Z"}, {1, "…T01:00Z"}]     <- frames sent once, deduped
//!                                                     across every node
//! series:
//!   app  -> samples: [{frame: 0, values: [10, 100]},
//!                     {frame: 1, values: [null, 120]}]
//!   util -> samples: [{frame: 1, values: [3, null]}]
//!           ^ frame 1 referenced by both nodes, stored once
//! ```
//!
//! `values` is positionally aligned with `metrics`, and `null` means the node
//! had no value for that metric at that frame. A sample whose values are *all*
//! null is history's record that the node was absent from that frame entirely
//! — a real event, not a gap in the data.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_db::HistorySeriesRow;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetHistoryInput {
    pub timeline_id: TimelineID,
    /// Nodes to read. Must be non-empty — history is far too large to return
    /// a whole timeline's worth unfiltered.
    pub node_names: Vec<String>,
    /// Inclusive lower bound on sample timestamp, RFC3339 (e.g. `2026-08-05T16:00:00Z`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_start: Option<String>,
    /// Inclusive upper bound on sample timestamp, RFC3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetHistoryOutput {
    /// Metric names in the order every `HistorySample::values` is aligned to.
    pub metrics: Vec<String>,
    /// Every frame referenced by `series`, deduplicated across nodes and
    /// sorted by `(timestamp, graph_id)`.
    pub frames: Vec<HistoryFrame>,
    /// One entry per requested node, sorted by name. A node with no recorded
    /// history still gets an entry, with no samples.
    pub series: Vec<NodeHistory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct HistoryFrame {
    pub graph_id: i64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct NodeHistory {
    pub node_name: String,
    pub samples: Vec<HistorySample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct HistorySample {
    /// Index into [`GetHistoryOutput::frames`].
    pub frame: i64,
    /// Values aligned with [`GetHistoryOutput::metrics`]. `null` where the
    /// node had no value for that metric at this frame; all-null means the
    /// node was absent from the frame.
    pub values: Vec<Option<f64>>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for GetHistoryInput {
    type Output = GetHistoryOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<GetHistoryOutput> {
        anyhow::ensure!(
            !self.node_names.is_empty(),
            "node_names must not be empty — history reads are scoped to specific nodes"
        );

        let bounds = to_timestamp_bounds(
            self.timestamp_start.as_deref(),
            self.timestamp_end.as_deref(),
        )?;
        let series = ctx
            .db
            .graph_history
            .series_many(&self.timeline_id, &self.node_names, &bounds, task)
            .await?;

        Ok(to_output(series))
    }
}

fn to_timestamp_bounds(start: Option<&str>, end: Option<&str>) -> Result<TimestampBounds> {
    Ok(TimestampBounds {
        start: start.map(Timestamp::from_rfc3339).transpose()?,
        end: end.map(Timestamp::from_rfc3339).transpose()?,
    })
}

/// Fold the per-node rows into the columnar wire shape.
fn to_output(series: BTreeMap<String, Vec<HistorySeriesRow>>) -> GetHistoryOutput {
    let metrics = collect_metric_names(&series);
    let frames = collect_frames(&series);
    let frame_indices = index_frames(&frames);

    let series = series
        .into_iter()
        .map(|(node_name, rows)| to_node_history(node_name, rows, &metrics, &frame_indices))
        .collect();

    GetHistoryOutput {
        metrics,
        frames,
        series,
    }
}

/// The union of every metric name appearing in any sample, sorted.
fn collect_metric_names(series: &BTreeMap<String, Vec<HistorySeriesRow>>) -> Vec<String> {
    series
        .values()
        .flatten()
        .flat_map(|row| row.values.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The union of every frame any node has a sample at, sorted by
/// `(timestamp, graph_id)` to match the timeline's own ordering.
fn collect_frames(series: &BTreeMap<String, Vec<HistorySeriesRow>>) -> Vec<HistoryFrame> {
    series
        .values()
        .flatten()
        .map(|row| (row.timestamp, row.graph_id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|(timestamp, graph_id)| HistoryFrame {
            graph_id: graph_id.0,
            timestamp: timestamp.to_rfc3339(),
        })
        .collect()
}

/// `graph_id → position in the frame table`. `graph_id` is unique within a
/// timeline, so it identifies a frame on its own.
fn index_frames(frames: &[HistoryFrame]) -> HashMap<GraphID, i64> {
    frames
        .iter()
        .enumerate()
        .map(|(idx, frame)| (GraphID(frame.graph_id), idx as i64))
        .collect()
}

fn to_node_history(
    node_name: String,
    rows: Vec<HistorySeriesRow>,
    metrics: &[String],
    frame_indices: &HashMap<GraphID, i64>,
) -> NodeHistory {
    let samples = rows
        .into_iter()
        .map(|row| HistorySample {
            frame: *frame_indices
                .get(&row.graph_id)
                .expect("should be present: the frame table is the union of these same rows"),
            values: metrics.iter().map(|m| row.values.get(m).copied()).collect(),
        })
        .collect();

    NodeHistory { node_name, samples }
}

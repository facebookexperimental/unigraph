// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Metric history write and read paths.
//!
//! # Write path
//!
//! History storage happens in three phases, split across the transaction
//! boundary for correctness:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ BEFORE TRANSACTION                                              │
//! │                                                                 │
//! │  1. prepare_history_entries()                                    │
//! │     Extract per-node metrics from graph(s), group by ISO week   │
//! │                                                                 │
//! │  2. ensure_history_partitions()                                  │
//! │     INSERT OR IGNORE empty rows for (timeline, week, node)      │
//! │     ⚠ Must happen BEFORE the transaction — MySQL has a          │
//! │     15-year-old bug where it gives away multiple locks for      │
//! │     non-existent rows within a transaction.                     │
//! ├─────────────────────────────────────────────────────────────────┤
//! │ INSIDE TRANSACTION (same txn as graph frame storage)            │
//! │                                                                 │
//! │  3. store_metric_history_on_conn()                              │
//! │     For each week partition:                                    │
//! │     a. Fetch ALL existing blobs for (timeline, week)            │
//! │     b. Fetch existing frames → build all_frames set             │
//! │     c. Insert missing frames (None for absent nodes)            │
//! │     d. Per-node (rayon parallel + spawn_blocking):              │
//! │        - Deserialize existing FlatHistory                       │
//! │        - Call FlatHistory::insert(entries, all_frames)           │
//! │        - Serialize updated FlatHistory                          │
//! │     e. Batch-upsert all updated blobs                           │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Absent node handling
//!
//! When a new graph is stored, some nodes that existed in previous graphs
//! may not be in the new graph. We must explicitly record their absence:
//!
//! ```text
//! Existing history blobs: ["Button", "Header", "Footer"]
//! New graph's nodes:      ["Button", "Header"]
//!                                                ^^^^^^^ "Footer" is missing!
//!
//! Without tracking:
//!   Footer's history would show its last known metrics persisting
//!   through this frame — incorrect.
//!
//! With tracking:
//!   We insert (graph_id, (timestamp, None)) for Footer, which records
//!   that Footer's metrics dropped to zero at this frame.
//! ```
//!
//! Similarly, within a batch of graphs, a node may appear in some graphs
//! but not others. We ensure every node has an entry for every frame in
//! the batch (filling `None` for frames where the node is absent).
//!
//! ## Performance
//!
//! Graphs can have tens of thousands of nodes. The write path is designed
//! for this scale:
//!
//! - **Batch DB operations**: fetch all week's blobs in one query, write
//!   all back in one batch — not one query per node.
//! - **rayon parallelism**: per-node delta computation and serialization
//!   runs on the rayon thread pool via `into_par_iter()`.
//! - **spawn_blocking**: all CPU-heavy work (deserialize + insert + serialize)
//!   runs inside `tokio::task::spawn_blocking()` to avoid blocking the
//!   async runtime.
//!
//! # Read path
//!
//! ```text
//! fetch_metric_history(timeline, node_names, start, end)
//!   │
//!   ├─ 1. Compute week range from start/end timestamps
//!   ├─ 2. Fetch blobs: get_metric_history_range(timeline, nodes, weeks)
//!   ├─ 3. spawn_blocking + rayon:
//!   │     For each node (parallel):
//!   │       For each week's blob:
//!   │         - Decompress + deserialize FlatHistory
//!   │         - Reconstruct entries via to_entries()
//!   │         - Filter to [start, end] time range
//!   │       Merge entries across weeks (concat + sort + dedup by GraphID)
//!   └─ 4. Return BTreeMap<NodeName, Vec<(Timestamp, GraphID, Snapshot)>>
//! ```
//!
//! Cross-week merging is straightforward: each week's FlatHistory is
//! independently reconstructed, entries are concatenated per node, sorted
//! by (Timestamp, GraphID), and deduplicated. No cross-week delta
//! dependencies exist — each partition starts with an absolute first frame.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use rayon::prelude::*;
use unigraph_core::ArrayGraphSerializable;
use unigraph_metric_history::FlatHistory;
use unigraph_metric_history::NodeMetricSnapshot;
use unigraph_metric_history::WeekPartition;
use unigraph_metric_history::extract_node_metrics;
use unigraph_metric_history::types::Frame;
use unigraph_metric_history::types::MetricHistoryEntries;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::UnigraphGraphConnection;
use unigraph_timestamp::Timestamp;

/// Per-week prepared history data, ready to be stored in a transaction.
///
/// Contains the extracted metrics for all nodes in all graphs that fall
/// within this ISO week. Built by [`prepare_history_entries`] before the
/// transaction starts.
pub struct PreparedHistoryForWeek {
    /// ISO week key in "YYYY-Www" format (e.g. "2025-W03").
    pub week_key: String,
    /// All unique node names that appear in any graph for this week.
    /// Used for partition pre-creation.
    pub all_node_names: Vec<String>,
    /// The (Timestamp, GraphID) pairs for each graph in this week.
    /// Used to build the `all_frames` set for `FlatHistory::insert()`.
    pub new_frames: Vec<Frame>,
    /// Per-node metric entries: `node_name → {graph_id → (timestamp, metrics)}`.
    /// `None` metrics means the node is absent from that graph.
    pub entries_by_node: BTreeMap<String, MetricHistoryEntries>,
}

/// Prepared history entries from one or more graphs, grouped by ISO week.
///
/// This is the output of [`prepare_history_entries`] and the input to
/// [`store_metric_history_on_conn`]. Separating preparation from storage
/// allows the expensive metric extraction to happen before the transaction,
/// while the actual DB writes happen inside it.
pub struct PreparedHistoryEntries {
    /// Keyed by ISO week key ("YYYY-Www").
    pub by_week: BTreeMap<String, PreparedHistoryForWeek>,
}

// --- Public API ---

/// Prepare metric history entries from one or more graphs.
///
/// Extracts per-node metrics from each graph, groups entries by ISO week
/// partition, and builds the per-node entry maps. This is the "Phase 1"
/// of the write path — pure computation, no DB access.
///
/// Call this **before** the transaction. Pass the result to
/// [`ensure_history_partitions`] and then [`store_metric_history_on_conn`].
///
/// Accepts a slice of `(key, graph)` pairs to support future batch
/// graph storage. For single-graph stores, pass a one-element slice.
pub fn prepare_history_entries(
    graphs: &[(GraphTimeKey, &ArrayGraphSerializable)],
) -> PreparedHistoryEntries {
    let mut by_week: BTreeMap<String, PreparedHistoryForWeek> = BTreeMap::new();

    for (key, graph) in graphs {
        let week = WeekPartition::from_timestamp(key.timestamp);
        let week_key = week.display_key();
        let node_metrics = extract_node_metrics(graph);

        let week_entry =
            by_week
                .entry(week_key.clone())
                .or_insert_with(|| PreparedHistoryForWeek {
                    week_key,
                    all_node_names: Vec::new(),
                    new_frames: Vec::new(),
                    entries_by_node: BTreeMap::new(),
                });

        week_entry.new_frames.push((key.timestamp, key.graph_id));

        for (node_name, snapshot) in node_metrics {
            week_entry
                .entries_by_node
                .entry(node_name)
                .or_default()
                .insert(key.graph_id, (key.timestamp, Some(snapshot)));
        }
    }

    // Collect all node names per week.
    for week_entry in by_week.values_mut() {
        let names: BTreeSet<String> = week_entry.entries_by_node.keys().cloned().collect();
        week_entry.all_node_names = names.into_iter().collect();
    }

    PreparedHistoryEntries { by_week }
}

/// Ensure partition rows exist in the DB. Call **before** the transaction.
///
/// Creates empty placeholder rows in the `metric_history` table for each
/// `(timeline, week, node)` combination using `INSERT OR IGNORE`.
///
/// This is a workaround for a MySQL row-locking bug: MySQL can give away
/// multiple locks for the same non-existent row within a transaction. By
/// ensuring the rows exist before starting the transaction, we guarantee
/// that `SELECT FOR UPDATE` (or SQLite's `BEGIN EXCLUSIVE`) properly
/// serializes concurrent writers.
///
/// For SQLite this is technically unnecessary (BEGIN EXCLUSIVE already
/// serializes), but we keep the pattern for portability to MySQL backends.
pub async fn ensure_history_partitions(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    prepared: &PreparedHistoryEntries,
) -> Result<()> {
    for week_entry in prepared.by_week.values() {
        conn.ensure_metric_history_partitions_exist(
            timeline_id,
            &week_entry.week_key,
            &week_entry.all_node_names,
        )
        .await?;
    }
    Ok(())
}

/// Store metric history within an existing transaction.
///
/// **The caller must already be inside a transaction.** This function does
/// NOT start or commit a transaction — the caller controls that. This
/// ensures history is committed atomically with the graph frame.
///
/// For each week partition:
/// 1. Fetch ALL existing history blobs for `(timeline, week)` — single query
/// 2. Fetch existing timeline frames → build the `all_frames` set
/// 3. Insert missing frames: for nodes in existing blobs but NOT in the new
///    graphs, add explicit `None` entries (absent node tracking)
/// 4. For each node (rayon parallel via `spawn_blocking`):
///    - Deserialize existing `FlatHistory` (or create empty)
///    - Call `FlatHistory::insert(entries, all_frames)`
///    - Serialize updated `FlatHistory` to ZSTD bytes
/// 5. Batch-upsert all updated blobs — single write query
pub async fn store_metric_history_on_conn(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    mut prepared: PreparedHistoryEntries,
) -> Result<()> {
    for (_week_key, week_entry) in &mut prepared.by_week {
        store_week(conn, timeline_id, week_entry).await?;
    }
    Ok(())
}

/// Fetch metric history for specific nodes within a time range.
///
/// Returns `BTreeMap<NodeName, Vec<(Timestamp, GraphID, Snapshot)>>` where
/// entries are sorted by `(Timestamp, GraphID)` and deduplicated.
///
/// The read path:
/// 1. Computes the ISO week range covering `[start, end]`
/// 2. Fetches all relevant blobs via `get_metric_history_range`
/// 3. Deserializes and reconstructs on a blocking thread (CPU-heavy):
///    - Each node's weekly blobs are independently reconstructed
///    - Entries are filtered to the `[start, end]` time range
///    - Entries from multiple weeks are merged: concat, sort, dedup by GraphID
/// 4. Returns only nodes that have at least one entry with `Some` metrics
///
/// **Cross-week merging**: Each weekly partition is self-contained (starts
/// with an absolute first frame). There are no cross-week delta dependencies,
/// so merging is a simple concatenation + sort.
pub async fn fetch_metric_history(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    node_names: &[String],
    start: Timestamp,
    end: Timestamp,
) -> Result<BTreeMap<String, Vec<(Timestamp, GraphID, NodeMetricSnapshot)>>> {
    let start_week = WeekPartition::from_timestamp(start).display_key();
    let end_week = WeekPartition::from_timestamp(end).display_key();

    let raw_blobs = conn
        .get_metric_history_range(timeline_id, node_names, &start_week, &end_week)
        .await?;

    if raw_blobs.is_empty() {
        return Ok(BTreeMap::new());
    }

    // Deserialize + reconstruct on a blocking thread (CPU-heavy).
    let start_unix = start.to_unix_timestamp();
    let end_unix = end.to_unix_timestamp();

    let result =
        tokio::task::spawn_blocking(move || deserialize_and_merge(raw_blobs, start_unix, end_unix))
            .await
            .context("spawn_blocking panicked")??;

    Ok(result)
}

// --- Internals ---

async fn store_week(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    week_entry: &mut PreparedHistoryForWeek,
) -> Result<()> {
    // 1. Fetch existing blobs for this week.
    let existing_blobs = conn
        .get_metric_history_for_week(timeline_id, &week_entry.week_key)
        .await?;

    // 2. Fetch existing frames for this timeline + week to build all_frames.
    let week_partition = WeekPartition::parse(&week_entry.week_key)?;
    let all_frames =
        build_all_frames(conn, timeline_id, &week_partition, &week_entry.new_frames).await?;

    // 3. Insert missing frames: for nodes in existing blobs but NOT in the
    //    new graphs, add explicit None entries.
    for existing_node_name in existing_blobs.keys() {
        if !week_entry.entries_by_node.contains_key(existing_node_name) {
            // Node exists in history but is absent from new graphs.
            let mut none_entries = MetricHistoryEntries::new();
            for &(ts, graph_id) in &week_entry.new_frames {
                none_entries.insert(graph_id, (ts, None));
            }
            week_entry
                .entries_by_node
                .insert(existing_node_name.clone(), none_entries);
        }
    }

    // Also, for nodes in new graphs, ensure they have entries for ALL new frames
    // (some nodes may only appear in some of the batch's graphs).
    let all_node_names: Vec<String> = week_entry.entries_by_node.keys().cloned().collect();
    for node_name in &all_node_names {
        let node_entries = week_entry.entries_by_node.get_mut(node_name).unwrap();
        for &(ts, graph_id) in &week_entry.new_frames {
            node_entries.entry(graph_id).or_insert((ts, None));
        }
    }

    // 4. Parallel: deserialize, insert, serialize per-node.
    let entries_by_node = std::mem::take(&mut week_entry.entries_by_node);
    let all_frames_clone = all_frames.clone();

    let updated_blobs = tokio::task::spawn_blocking(move || -> Result<Vec<(String, Vec<u8>)>> {
        entries_by_node
            .into_par_iter()
            .map(|(node_name, new_entries)| {
                let mut history = match existing_blobs.get(&node_name) {
                    Some(blob) if !blob.is_empty() => FlatHistory::from_compressed_bytes(blob)
                        .with_context(|| {
                            format!("failed to deserialize history for node '{node_name}'")
                        })?,
                    _ => FlatHistory::default(),
                };

                history
                    .insert(new_entries, &all_frames_clone)
                    .with_context(|| format!("failed to insert history for node '{node_name}'"))?;

                let blob = history.to_compressed_bytes().with_context(|| {
                    format!("failed to serialize history for node '{node_name}'")
                })?;

                Ok((node_name, blob))
            })
            .collect()
    })
    .await
    .context("spawn_blocking panicked")??;

    // 5. Batch-upsert.
    conn.upsert_metric_history_batch(timeline_id, &week_entry.week_key, &updated_blobs)
        .await?;

    Ok(())
}

/// Build the `all_frames` set for a week: existing frames from DB + new frames.
async fn build_all_frames(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    _week_partition: &WeekPartition,
    new_frames: &[Frame],
) -> Result<BTreeSet<Frame>> {
    // Fetch existing Full/Delta frames for this timeline.
    // We fetch all of them since we need the complete frame set for
    // anchor materialization. In practice this is bounded by the
    // number of frames in the timeline.
    let existing = conn
        .select_frames(&FrameQuery {
            timeline_id: timeline_id.clone(),
            frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
            order: Some(Order::Asc),
            ..Default::default()
        })
        .await?;

    let mut all_frames: BTreeSet<Frame> = existing
        .iter()
        .map(|row| (row.frame.timestamp, row.frame.graph_id))
        .collect();

    for &(ts, graph_id) in new_frames {
        all_frames.insert((ts, graph_id));
    }

    Ok(all_frames)
}

/// Deserialize blobs, reconstruct entries, merge across weeks, filter by time range.
fn deserialize_and_merge(
    raw_blobs: Vec<(String, String, Vec<u8>)>,
    start_unix: i64,
    end_unix: i64,
) -> Result<BTreeMap<String, Vec<(Timestamp, GraphID, NodeMetricSnapshot)>>> {
    // Group by node_name, then process each node's blobs in parallel.
    let mut by_node: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    for (node_name, week_key, data) in raw_blobs {
        by_node.entry(node_name).or_default().push((week_key, data));
    }

    let result: BTreeMap<String, Vec<(Timestamp, GraphID, NodeMetricSnapshot)>> = by_node
        .into_par_iter()
        .filter_map(|(node_name, blobs)| {
            match reconstruct_node_history(blobs, start_unix, end_unix) {
                Ok(entries) if entries.is_empty() => None,
                Ok(entries) => Some(Ok((node_name, entries))),
                Err(e) => Some(Err(e)),
            }
        })
        .collect::<Result<_>>()?;

    Ok(result)
}

fn reconstruct_node_history(
    blobs: Vec<(String, Vec<u8>)>,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<(Timestamp, GraphID, NodeMetricSnapshot)>> {
    let mut all_entries: BTreeMap<(Timestamp, GraphID), NodeMetricSnapshot> = BTreeMap::new();

    for (_week_key, data) in blobs {
        let history = FlatHistory::from_compressed_bytes(&data)?;
        let entries = history.to_entries()?;

        for (_graph_id, (ts, snapshot)) in entries {
            let unix = ts.to_unix_timestamp();
            if unix < start_unix || unix > end_unix {
                continue;
            }
            if let Some(snapshot) = snapshot {
                // Use (ts, graph_id) as dedup key.
                all_entries.insert((ts, _graph_id), snapshot);
            }
        }
    }

    let result: Vec<_> = all_entries
        .into_iter()
        .map(|((ts, gid), snap)| (ts, gid, snap))
        .collect();

    Ok(result)
}

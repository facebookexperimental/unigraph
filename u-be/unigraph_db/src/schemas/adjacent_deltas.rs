// Copyright (c) Meta Platforms, Inc. and affiliates.

//! AdjacentDeltas timeline schema — iterative graph fetch and compaction.
//!
//! # Problem
//!
//! Unigraph stores graph snapshots as a sequence of frames in a timeline.
//! To save space, most frames are stored as *deltas* against the previous
//! frame rather than as full snapshots. A naive implementation resolves
//! delta chains recursively:
//!
//! ```text
//!   fetch(graph_id=5)
//!     → frame 5 is Delta(base=4), fetch(4)
//!       → frame 4 is Delta(base=3), fetch(3)
//!         → frame 3 is Delta(base=2), fetch(2)
//!           → ...
//!             → frame 0 is Full → return graph_0
//!           → apply delta_2 → return graph_2
//!         → apply delta_3 → return graph_3
//!       → apply delta_4 → return graph_4
//!     → apply delta_5 → return graph_5
//! ```
//!
//! This creates an O(N)-deep async call stack. After compaction, a timeline
//! with hundreds of frames becomes a delta chain hundreds of levels deep.
//! Tokio worker threads have a 2MB stack by default, causing stack overflow
//! at around 350 frames.
//!
//! # Solution: AdjacentDeltas schema
//!
//! This module enforces two invariants and provides an iterative fetch:
//!
//! ## Invariant 1: Monotonic append-only ordering
//!
//! New frames in a timeline must have strictly increasing `graph_id` and
//! non-decreasing `timestamp`. No out-of-order inserts are allowed.
//! This is enforced transactionally: lock timeline → validate → insert → commit.
//!
//! **Replacements** (e.g. Empty → Full, or Full → Delta during compaction)
//! are exempt from this check since they don't change a frame's position
//! in the timeline.
//!
//! ## Invariant 2: Adjacent delta base references
//!
//! Every Delta frame must reference the immediately preceding frame as its
//! base. This means the delta chain is always a simple linked list — no
//! jumps, no cross-timeline references:
//!
//! ```text
//!   OK:    Full ← Delta ← Delta ← Delta ← Delta
//!             0      1       2       3       4
//!
//!   OK:    Full ← Delta ← Full ← Delta ← Delta
//!             0      1       2      3       4
//!          (Full in the middle is fine — starts a new sub-chain)
//!
//!   BAD:   Full    Delta → Delta ← Delta ← Delta
//!             0      1  ↗    2       3       4
//!          (Delta 2 skips back to 0, not adjacent to 1)
//!
//!   BAD:   Full(timeline_a) ← Delta(timeline_b)
//!          (Cross-timeline references are not allowed)
//! ```
//!
//! ## Iterative fetch via range query
//!
//! With these invariants, fetching a graph is a flat three-step process:
//!
//! ```text
//!   Timeline:  [Full]  [Delta]  [Delta]  [Full]  [Delta]  [Delta]
//!   graph_id:     0       1        2        3       4        5
//!                                                         target
//!
//!   Step 1: Find nearest Full at or before target (graph_id=3)
//!           → metadata-only query, very cheap
//!
//!   Step 2: Load range [3..5] with data
//!           → single SQL query: Full(3), Delta(4), Delta(5)
//!
//!   Step 3: Reconstruct by folding:
//!           graph_3 = unpack(Full_3)
//!           graph_4 = apply_delta(graph_3, delta_4)
//!           graph_5 = apply_delta(graph_4, delta_5)  ← result
//! ```
//!
//! No recursion, no deep call stacks, O(1) SQL queries regardless of chain
//! length. Works for timelines with thousands of frames.
//!
//! # Performance notes
//!
//! Graphs can be very large (tens of MB). All functions in this module
//! are careful to:
//! - Never fetch frame data when only metadata is needed
//! - Never clone a graph unnecessarily
//! - Never fetch the same frame twice
//!
//! The `find_nearest_full_frame` step fetches metadata only (no data).
//! The `load_frame_range` step does a single query that fetches all needed
//! data in one pass, including the Full frame.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::apply_delta;
use unigraph_core::derive_delta;
use unigraph_core::pack_delta;
use unigraph_storage_core::Frame;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;
use unigraph_storage_core::UnigraphGraphConnection;

use crate::frame_storage::make_pack_config;
use crate::frame_storage::prepare_inline_blobs;
use crate::storage::UnigraphStorage;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Store a full graph snapshot in an AdjacentDeltas timeline.
///
/// Prepares metric history (if enabled), packs the graph, prepares blobs,
/// then stores everything in a single transaction with monotonic ordering
/// validation.
pub async fn store_full(
    storage: &UnigraphStorage,
    key: &GraphTimeKey,
    graph: &ArrayGraphSerializable,
) -> Result<()> {
    let prepared_history = storage.prepare_history_if_enabled(key, graph).await?;

    let config = make_pack_config(key);
    let package = graph.pack(&config).context("Failed to pack graph")?;
    let manifest_json =
        serde_json::to_string(&package.manifest).context("Failed to serialize graph manifest")?;

    let prepared = storage
        .prepare_blobs_for_storage(&key.timeline_id, &package.blobs)
        .await?;

    let mut conn = storage.graph.conn().await?;
    conn.start_transaction().await?;
    conn.get_timeline_config_and_lock(&key.timeline_id).await?;
    validate_monotonic_append(&mut *conn, key).await?;

    storage
        .store_package_on_conn(
            &mut *conn,
            key,
            FrameType::Full,
            None,
            &manifest_json,
            prepared.inline.as_deref(),
            prepared.external_keys.as_deref(),
        )
        .await?;

    if let Some(prepared_history) = prepared_history {
        crate::metric_history::store_metric_history_on_conn(
            &mut *conn,
            &key.timeline_id,
            prepared_history,
        )
        .await?;
    }

    conn.commit_transaction().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/// Fetch a graph from an AdjacentDeltas timeline without recursion.
///
/// See module-level docs for the algorithm.
pub async fn fetch_graph(
    storage: &UnigraphStorage,
    key: &GraphKey,
) -> Result<ArrayGraphSerializable> {
    let full_frame = find_nearest_full_frame(storage, key).await?;
    let range = load_frame_range(storage, key, full_frame.graph_id).await?;
    reconstruct_from_range(storage, &range).await
}

/// Find the most recent Full frame at or before the target graph_id.
///
/// Returns metadata only (no data) — the actual graph data is loaded
/// later in `load_frame_range` to avoid fetching it twice.
async fn find_nearest_full_frame(storage: &UnigraphStorage, key: &GraphKey) -> Result<Frame> {
    let mut conn = storage.graph.conn().await?;
    let mut rows = conn
        .select_frames(&FrameQuery {
            timeline_id: key.timeline_id.clone(),
            frame_types: Some(vec![FrameType::Full]),
            graph_id_bounds: Some((None, Some(key.graph_id))),
            order: Some(Order::Desc),
            limit: Some(1),
            ..Default::default()
        })
        .await?;

    let row = rows.pop().with_context(|| {
        format!(
            "no Full frame found at or before graph_id={} in timeline '{}'",
            key.graph_id.0, key.timeline_id.0,
        )
    })?;

    Ok(row.frame)
}

/// Load the contiguous range of frames from `full_graph_id` to the target,
/// inclusive on both ends, with data.
///
/// Always does a single SQL query, even if `full_graph_id == target`.
async fn load_frame_range(
    storage: &UnigraphStorage,
    key: &GraphKey,
    full_graph_id: GraphID,
) -> Result<FrameRange> {
    let mut conn = storage.graph.conn().await?;
    let rows = conn
        .select_frames(&FrameQuery {
            timeline_id: key.timeline_id.clone(),
            frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
            graph_id_bounds: Some((Some(full_graph_id), Some(key.graph_id))),
            order: Some(Order::Asc),
            with_data: Some(true),
            ..Default::default()
        })
        .await?;

    FrameRange::from_rows(rows, key.graph_id)
}

/// Reconstruct a graph from a validated frame range.
///
/// Finds the last Full frame in the range, reconstructs it, then applies
/// each subsequent delta sequentially to produce the target graph.
async fn reconstruct_from_range(
    storage: &UnigraphStorage,
    range: &FrameRange,
) -> Result<ArrayGraphSerializable> {
    let entries: Vec<_> = range.frames.values().collect();

    // Find the last Full frame (use that as the base, skip earlier frames).
    let last_full_idx = entries
        .iter()
        .rposition(|row| row.frame_type == FrameType::Full)
        .expect("validate ensures at least one Full frame");

    let full_row = &entries[last_full_idx];
    let data = full_row.data.as_ref().with_context(|| {
        format!(
            "full frame graph_id={} has no data",
            full_row.frame.graph_id.0
        )
    })?;

    let base_graph = storage.reconstruct_full_graph(data).await?;

    // If there are no deltas after the Full frame, we're done.
    let delta_rows = &entries[last_full_idx + 1..];
    if delta_rows.is_empty() {
        return Ok(base_graph);
    }

    // Apply each delta sequentially.
    // Each delta was derived from the preceding graph, so we fold:
    // base → apply(delta_1) → apply(delta_2) → ... → result
    let mut current = base_graph;
    for row in delta_rows {
        let data = row.data.as_ref().with_context(|| {
            format!("delta frame graph_id={} has no data", row.frame.graph_id.0)
        })?;
        let delta = storage.reconstruct_delta(data).await?;
        current = apply_delta(current, &delta).with_context(|| {
            format!("failed to apply delta at graph_id={}", row.frame.graph_id.0)
        })?;
    }

    Ok(current)
}

// ---------------------------------------------------------------------------
// Compaction
// ---------------------------------------------------------------------------

/// Compact a timeline by replacing consecutive Full frames with Deltas.
///
/// Walks frames in `(timestamp, graph_id)` order within the given range.
/// The first Full frame stays Full. Every subsequent Full is replaced with
/// a Delta derived from the previous data-carrying frame. Empty and Error
/// frames break the chain (the next Full after them stays Full).
///
/// ```text
///   Before:  [Full]  [Full]  [Full]  [Full]  [Full]
///               0       1       2       3       4
///
///   After:   [Full]  [Delta] [Delta] [Delta] [Delta]
///               0    base=0  base=1  base=2  base=3
/// ```
///
/// Returns the number of frames converted from Full to Delta.
pub async fn compact_timeline(
    storage: &UnigraphStorage,
    timeline_id: &TimelineID,
    start: Option<Timestamp>,
    end: Option<Timestamp>,
) -> Result<usize> {
    let mut conn = storage.graph.conn().await?;

    // Verify the timeline uses AdjacentDeltas schema.
    let config = conn
        .get_timeline_config(timeline_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
    anyhow::ensure!(
        matches!(config.schema, TimelineSchema::AdjacentDeltas(_)),
        "compact_timeline only supports AdjacentDeltas schema, got {:?}",
        config.schema,
    );

    let timestamp_bounds = if start.is_some() || end.is_some() {
        Some(TimestampBounds { start, end })
    } else {
        None
    };

    let frames = conn
        .select_frames(&FrameQuery {
            timeline_id: timeline_id.clone(),
            timestamp_bounds,
            ..Default::default()
        })
        .await?;
    drop(conn);

    let mut converted = 0;
    let mut prev_data_key: Option<GraphKey> = None;

    for frame in &frames {
        match frame.frame_type {
            FrameType::Full => {
                if let Some(base_key) = &prev_data_key {
                    replace_full_with_delta(storage, timeline_id, base_key, frame).await?;
                    converted += 1;
                }
                prev_data_key = Some(GraphKey {
                    timeline_id: timeline_id.clone(),
                    graph_id: frame.frame.graph_id,
                });
            }
            FrameType::Delta => {
                prev_data_key = Some(GraphKey {
                    timeline_id: timeline_id.clone(),
                    graph_id: frame.frame.graph_id,
                });
            }
            FrameType::Empty | FrameType::Error => {
                prev_data_key = None;
            }
        }
    }

    Ok(converted)
}

/// Replace a Full frame with a Delta derived from a base frame.
///
/// Fetches both graphs, derives the delta, packs it, and atomically
/// swaps the frame using `delete_frame_on_conn` + `store_package_on_conn`
/// in a single transaction.
async fn replace_full_with_delta(
    storage: &UnigraphStorage,
    timeline_id: &TimelineID,
    base_key: &GraphKey,
    target_frame: &FrameRow,
) -> Result<()> {
    let target_key = GraphKey {
        timeline_id: timeline_id.clone(),
        graph_id: target_frame.frame.graph_id,
    };
    let target_time_key = GraphTimeKey {
        timeline_id: timeline_id.clone(),
        timestamp: target_frame.frame.timestamp,
        graph_id: target_frame.frame.graph_id,
    };

    let base_graph = storage
        .fetch_graph(base_key)
        .await
        .with_context(|| format!("Failed to fetch base graph {:?}", base_key))?;
    let target_graph = storage
        .fetch_graph(&target_key)
        .await
        .with_context(|| format!("Failed to fetch target graph {:?}", target_key))?;

    let delta = derive_delta(&base_graph, &target_graph).context("Failed to derive delta")?;

    let config = make_pack_config(&target_time_key);
    let package = pack_delta(&delta, &config).context("Failed to pack delta")?;
    let manifest_json =
        serde_json::to_string(&package.manifest).context("Failed to serialize delta manifest")?;

    // Determine inline vs. external for the new delta.
    let threshold = {
        let mut conn = storage.graph.conn().await?;
        let config = conn
            .get_timeline_config(timeline_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
        config.inline_blob_threshold()
    };

    let inline_blobs = prepare_inline_blobs(&package.blobs, threshold)?;
    let blob_keys_to_unregister = if inline_blobs.is_none() {
        Some(storage.upload_blobs(&package.blobs).await?)
    } else {
        None
    };

    // Single transaction: delete old Full + insert new Delta.
    let mut conn = storage.graph.conn().await?;
    conn.start_transaction().await?;
    conn.get_timeline_config_and_lock(timeline_id).await?;

    storage
        .delete_frame_on_conn(&mut *conn, &target_key)
        .await?;
    storage
        .store_package_on_conn(
            &mut *conn,
            &target_time_key,
            FrameType::Delta,
            Some(base_key),
            &manifest_json,
            inline_blobs.as_deref(),
            blob_keys_to_unregister.as_deref(),
        )
        .await?;

    conn.commit_transaction().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Store validation
// ---------------------------------------------------------------------------

/// Validate that a new frame maintains monotonic `(timestamp, graph_id)` ordering.
///
/// Must be called inside an exclusive transaction on the timeline.
///
/// Two cases:
/// - **New frame** (no existing frame with this graph_id): checks that
///   `graph_id` is strictly greater than the current last frame and
///   `timestamp` is non-decreasing.
/// - **Replacement** (a frame with this graph_id already exists, e.g.
///   Empty → Full or Full → Delta): skips the append check since the
///   frame's position in the timeline is unchanged.
pub(crate) async fn validate_monotonic_append(
    conn: &mut dyn UnigraphGraphConnection,
    key: &GraphTimeKey,
) -> Result<()> {
    // Check if a frame with this graph_id already exists (replacement).
    let existing = conn
        .select_frames(&FrameQuery {
            timeline_id: key.timeline_id.clone(),
            graph_ids: Some(vec![key.graph_id]),
            limit: Some(1),
            ..Default::default()
        })
        .await?;

    if !existing.is_empty() {
        return Ok(()); // replacing existing frame — ordering unchanged
    }

    // New frame — enforce append-only ordering.
    let mut rows = conn
        .select_frames(&FrameQuery {
            timeline_id: key.timeline_id.clone(),
            order: Some(Order::Desc),
            limit: Some(1),
            ..Default::default()
        })
        .await?;

    let last = match rows.pop() {
        Some(row) => row,
        None => return Ok(()), // first frame in timeline — no constraints
    };

    anyhow::ensure!(
        key.graph_id.0 > last.frame.graph_id.0,
        "monotonic ordering violated: new graph_id={} must be greater than \
         last graph_id={} in timeline '{}'",
        key.graph_id.0,
        last.frame.graph_id.0,
        key.timeline_id.0,
    );

    anyhow::ensure!(
        key.timestamp >= last.frame.timestamp,
        "monotonic ordering violated: new timestamp={} must be >= \
         last timestamp={} in timeline '{}' (graph_id={} vs {})",
        key.timestamp,
        last.frame.timestamp,
        key.timeline_id.0,
        key.graph_id.0,
        last.frame.graph_id.0,
    );

    Ok(())
}

/// Validate that a Delta frame's base key references the immediately preceding frame.
///
/// Must be called inside an exclusive transaction on the timeline.
#[allow(dead_code)]
pub(crate) async fn validate_delta_base(
    conn: &mut dyn UnigraphGraphConnection,
    key: &GraphTimeKey,
    base: Option<&GraphKey>,
) -> Result<()> {
    let base = base.ok_or_else(|| {
        anyhow::anyhow!("delta frame graph_id={} has no base key", key.graph_id.0,)
    })?;

    // The preceding frame is the one with the highest graph_id less than ours.
    let mut rows = conn
        .select_frames(&FrameQuery {
            timeline_id: key.timeline_id.clone(),
            graph_id_bounds: Some((None, Some(GraphID(key.graph_id.0 - 1)))),
            order: Some(Order::Desc),
            limit: Some(1),
            ..Default::default()
        })
        .await?;

    let prev = rows.pop().with_context(|| {
        format!(
            "delta frame graph_id={} has no preceding frame in timeline '{}'",
            key.graph_id.0, key.timeline_id.0,
        )
    })?;

    anyhow::ensure!(
        base.graph_id == prev.frame.graph_id,
        "delta frame graph_id={} base points to graph_id={}, \
         but the preceding frame is graph_id={} in timeline '{}'",
        key.graph_id.0,
        base.graph_id.0,
        prev.frame.graph_id.0,
        key.timeline_id.0,
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// FrameRange
// ---------------------------------------------------------------------------

/// A validated range of frames from an AdjacentDeltas timeline.
///
/// Contains exactly the frames needed to reconstruct a target graph:
/// one or more Full frames and zero or more Delta frames, all in
/// monotonically increasing `(timestamp, graph_id)` order.
///
/// ```text
///   Example range for fetching graph_id=5:
///
///   BTreeMap keys (Frame order):
///     Frame(ts=100, id=3)  →  FrameRow { type: Full,  data: Some(...) }
///     Frame(ts=101, id=4)  →  FrameRow { type: Delta, data: Some(...), base: 3 }
///     Frame(ts=102, id=5)  →  FrameRow { type: Delta, data: Some(...), base: 4 }
/// ```
pub struct FrameRange {
    /// Frames in order, keyed by Frame (monotonically increasing).
    /// First entry must be Full. Subsequent entries are Delta (derived
    /// from the immediately preceding frame) or Full.
    pub frames: BTreeMap<Frame, FrameRow>,
}

impl FrameRange {
    /// Build a frame range from query results and validate the chain.
    pub fn from_rows(rows: Vec<FrameRow>, target_graph_id: GraphID) -> Result<Self> {
        let frames: BTreeMap<Frame, FrameRow> = rows
            .into_iter()
            .map(|row| (row.frame.clone(), row))
            .collect();

        let range = Self { frames };
        range.validate(target_graph_id)?;
        Ok(range)
    }

    /// Validate that this range forms a well-linked chain for reconstruction.
    fn validate(&self, target_graph_id: GraphID) -> Result<()> {
        anyhow::ensure!(!self.frames.is_empty(), "frame range is empty");

        let entries: Vec<_> = self.frames.values().collect();

        // First frame must be Full.
        anyhow::ensure!(
            entries[0].frame_type == FrameType::Full,
            "first frame in range must be Full, got {:?} (graph_id={})",
            entries[0].frame_type,
            entries[0].frame.graph_id.0,
        );

        // Each subsequent frame must be Delta or Full, no Empty/Error.
        // Delta frames must reference the immediately preceding frame.
        for i in 1..entries.len() {
            let prev = &entries[i - 1];
            let curr = &entries[i];

            match &curr.frame_type {
                FrameType::Delta => {
                    let base = curr.base.as_ref().with_context(|| {
                        format!(
                            "delta frame graph_id={} has no base key",
                            curr.frame.graph_id.0
                        )
                    })?;
                    anyhow::ensure!(
                        base.graph_id == prev.frame.graph_id,
                        "delta frame graph_id={} has base graph_id={}, \
                         expected graph_id={} (the preceding frame)",
                        curr.frame.graph_id.0,
                        base.graph_id.0,
                        prev.frame.graph_id.0,
                    );
                }
                FrameType::Full => {
                    // A Full frame in the middle is allowed — it starts a new chain.
                }
                other => {
                    anyhow::bail!(
                        "unexpected {:?} frame in range at graph_id={} \
                         (only Full and Delta are allowed)",
                        other,
                        curr.frame.graph_id.0,
                    );
                }
            }
        }

        // Last frame must be our target.
        let last = entries.last().unwrap();
        anyhow::ensure!(
            last.frame.graph_id == target_graph_id,
            "last frame in range has graph_id={}, expected target graph_id={}",
            last.frame.graph_id.0,
            target_graph_id.0,
        );

        Ok(())
    }
}

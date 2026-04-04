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

mod cas_store;
mod load_range;

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
pub use cas_store::store_range;
pub use load_range::load_range;
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
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;
use unigraph_storage_core::UnigraphGraphConnection;

use crate::context::UnigraphDbContext;
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
    ctx: &UnigraphDbContext,
    key: &GraphTimeKey,
    graph: &ArrayGraphSerializable,
    expires_at: Option<Timestamp>,
    task: &ll::Task,
) -> Result<()> {
    let storage = &ctx.storage;
    let prepared_history = storage.prepare_history_if_enabled(key, graph, task).await?;

    let config = ctx.pack_config_for_key(key);
    let package = graph.pack(&config).context("Failed to pack graph")?;
    let manifest_json =
        serde_json::to_string(&package.manifest).context("Failed to serialize graph manifest")?;

    let prepared = storage
        .prepare_blobs_for_storage(&key.timeline_id, &package.blobs, task)
        .await?;

    let mut conn = storage.graph.conn_write().await?;
    conn.start_transaction(task).await?;
    conn.get_timeline_config_and_lock(&key.timeline_id, task)
        .await?;
    validate_monotonic_append(&mut *conn, key, task).await?;

    storage
        .store_package_on_conn(
            &mut *conn,
            key,
            FrameType::Full,
            None,
            &manifest_json,
            prepared.inline.as_deref(),
            prepared.external_keys.as_deref(),
            expires_at,
            task,
        )
        .await?;

    if let Some(prepared_history) = prepared_history {
        crate::metric_history::store_metric_history_on_conn(
            &mut *conn,
            &key.timeline_id,
            prepared_history,
            task,
        )
        .await?;
    }

    conn.commit_transaction(task).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/// Fetch a graph from an AdjacentDeltas timeline without recursion.
///
/// See module-level docs for the algorithm.
#[ll::task]
pub async fn fetch_graph(
    storage: &UnigraphStorage,
    key: &GraphKey,
    task: &ll::Task,
) -> Result<ArrayGraphSerializable> {
    task.data("graph_key", key.to_string());
    let full_frame = find_nearest_full_frame(storage, key, &task).await?;
    let range = load_frame_range(storage, key, full_frame.graph_id, &task).await?;
    reconstruct_from_range(storage, &range, &task).await
}

/// Find the most recent Full frame at or before the target graph_id.
///
/// Returns metadata only (no data) — the actual graph data is loaded
/// later in `load_frame_range` to avoid fetching it twice.
async fn find_nearest_full_frame(
    storage: &UnigraphStorage,
    key: &GraphKey,
    task: &ll::Task,
) -> Result<Frame> {
    let mut conn = storage.graph.conn().await?;
    let mut rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                frame_types: Some(vec![FrameType::Full]),
                graph_id_bounds: Some((None, Some(key.graph_id))),
                order: Some(Order::Desc),
                limit: Some(1),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
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
    task: &ll::Task,
) -> Result<FrameRange> {
    let mut conn = storage.graph.conn().await?;
    let rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                graph_id_bounds: Some((Some(full_graph_id), Some(key.graph_id))),
                order: Some(Order::Asc),
                with_data: Some(true),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
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
    task: &ll::Task,
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

    let base_graph = storage.reconstruct_full_graph(data, task).await?;

    // If there are no deltas after the Full frame, we're done.
    let delta_rows = &entries[last_full_idx + 1..];
    if delta_rows.is_empty() {
        return Ok(base_graph);
    }

    // Prefetch all deltas in parallel (blob resolution + deserialization).
    let delta_futs: Vec<_> = delta_rows
        .iter()
        .map(|row| async {
            let data = row.data.as_ref().with_context(|| {
                format!("delta frame graph_id={} has no data", row.frame.graph_id.0)
            })?;
            let delta = storage.reconstruct_delta(data, task).await?;
            Ok::<_, anyhow::Error>((row.frame.graph_id, delta))
        })
        .collect();
    let prefetched = futures::future::try_join_all(delta_futs).await?;

    // Apply deltas sequentially on a blocking thread (CPU-heavy).
    let current = tokio::task::spawn_blocking(move || {
        let mut current = base_graph;
        for (graph_id, delta) in &prefetched {
            current = apply_delta(current, delta)
                .with_context(|| format!("failed to apply delta at graph_id={}", graph_id.0))?;
        }
        Ok::<_, anyhow::Error>(current)
    })
    .await
    .context("spawn_blocking panicked")??;

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
    ctx: &UnigraphDbContext,
    timeline_id: &TimelineID,
    start: Option<Timestamp>,
    end: Option<Timestamp>,
    task: &ll::Task,
) -> Result<usize> {
    let storage = &ctx.storage;
    let mut conn = storage.graph.conn().await?;

    // Verify the timeline uses AdjacentDeltas schema.
    let config = conn
        .get_timeline_config(timeline_id, task)
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
        .select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                timestamp_bounds,
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
        .await?;
    drop(conn);

    let mut converted = 0;
    let mut prev_data_key: Option<GraphKey> = None;

    for frame in &frames {
        match frame.frame_type {
            FrameType::Full => {
                if let Some(base_key) = &prev_data_key {
                    replace_full_with_delta(ctx, timeline_id, base_key, frame, task).await?;
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
    ctx: &UnigraphDbContext,
    timeline_id: &TimelineID,
    base_key: &GraphKey,
    target_frame: &FrameRow,
    task: &ll::Task,
) -> Result<()> {
    let storage = &ctx.storage;
    let target_key = GraphKey {
        timeline_id: timeline_id.clone(),
        graph_id: target_frame.frame.graph_id,
    };
    let target_time_key = GraphTimeKey {
        timeline_id: timeline_id.clone(),
        timestamp: target_frame.frame.timestamp,
        graph_id: target_frame.frame.graph_id,
    };

    let (base_graph, target_graph) = tokio::try_join!(
        storage.fetch_graph(base_key, task),
        storage.fetch_graph(&target_key, task),
    )
    .with_context(|| {
        format!(
            "Failed to fetch base {:?} or target {:?}",
            base_key, target_key
        )
    })?;

    // CPU-heavy: derive delta + pack → off the tokio thread
    let config = ctx.pack_config_for_key(&target_time_key);
    let (package, manifest_json) = tokio::task::spawn_blocking(move || {
        let delta = derive_delta(&base_graph, &target_graph).context("Failed to derive delta")?;
        let package = pack_delta(&delta, &config).context("Failed to pack delta")?;
        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize delta manifest")?;
        Ok::<_, anyhow::Error>((package, manifest_json))
    })
    .await
    .context("spawn_blocking panicked")??;

    // Determine inline vs. external for the new delta.
    let threshold = {
        let mut conn = storage.graph.conn().await?;
        let config = conn
            .get_timeline_config(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
        config.inline_blob_threshold()
    };

    let inline_blobs = prepare_inline_blobs(&package.blobs, threshold)?;
    let blob_keys_to_unregister = if inline_blobs.is_none() {
        Some(storage.upload_blobs(&package.blobs, task).await?)
    } else {
        None
    };

    // Single transaction: delete old Full + insert new Delta.
    let mut conn = storage.graph.conn_write().await?;
    conn.start_transaction(task).await?;
    conn.get_timeline_config_and_lock(timeline_id, task).await?;

    storage
        .delete_frame_on_conn(&mut *conn, &target_key, task)
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
            None,
            task,
        )
        .await?;

    conn.commit_transaction(task).await?;
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
    task: &ll::Task,
) -> Result<()> {
    // Check if a frame with this graph_id already exists (replacement).
    let existing = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                graph_ids: Some(vec![key.graph_id]),
                limit: Some(1),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
        .await?;

    if !existing.is_empty() {
        return Ok(()); // replacing existing frame — ordering unchanged
    }

    // New frame — enforce append-only ordering.
    let mut rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                order: Some(Order::Desc),
                limit: Some(1),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
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
    task: &ll::Task,
) -> Result<()> {
    let base = base.ok_or_else(|| {
        anyhow::anyhow!("delta frame graph_id={} has no base key", key.graph_id.0,)
    })?;

    // The preceding frame is the one with the highest graph_id less than ours.
    let mut rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                graph_id_bounds: Some((None, Some(GraphID(key.graph_id.0 - 1)))),
                order: Some(Order::Desc),
                limit: Some(1),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
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
// Batch empty frame registration
// ---------------------------------------------------------------------------

/// Maximum number of empty frames to INSERT in a single batch within the
/// transaction. Keeps individual SQL statements bounded.
const EMPTY_FRAME_CHUNK_SIZE: usize = 10_000;

/// Maximum number of stored frames to load for overlap resolution.
/// Only the tail of the timeline is needed — the overlap region is always
/// a suffix of stored frames matching a prefix of the input.
const STORED_FRAMES_LOOKBACK: i64 = 10_000;

/// Store new empty frames, filtering out frames already present in the timeline.
///
/// `new_frames` must be sorted by `(timestamp, graph_id)` with strictly
/// increasing graph_ids and non-decreasing timestamps. The range may
/// overlap with the tail of already-stored frames — the overlap is
/// validated for alignment and filtered out.
///
/// When `require_overlap` is `true`, the input MUST overlap with at least
/// one stored frame (unless the timeline is empty). This prevents silently
/// appending frames with a gap when the caller expected continuity.
///
/// Runs in a single transaction with an exclusive timeline lock.
/// Inserts are chunked to keep SQL statements bounded.
///
/// Returns the number of frames actually inserted (after filtering overlap).
pub async fn put_new_empty_frames(
    ctx: &UnigraphDbContext,
    timeline_id: &TimelineID,
    new_frames: Vec<Frame>,
    require_overlap: bool,
    task: &ll::Task,
) -> Result<usize> {
    if new_frames.is_empty() {
        return Ok(0);
    }

    validate_input_ordering(&new_frames)?;

    let mut conn = ctx.storage.graph.conn_write().await?;
    conn.start_transaction(task).await?;
    let config = lock_timeline(&mut *conn, timeline_id, task).await?;
    ensure_adjacent_deltas_schema(&config, timeline_id)?;

    let stored_tail = load_stored_tail(&mut *conn, timeline_id, task).await?;
    let to_insert = resolve_overlap(&stored_tail, &new_frames, require_overlap)?;

    if to_insert.is_empty() {
        conn.commit_transaction(task).await?;
        return Ok(0);
    }

    validate_monotonic_continuation(&stored_tail, &to_insert[0], timeline_id)?;
    insert_empty_frames_chunked(&mut *conn, timeline_id, to_insert, task).await?;

    conn.commit_transaction(task).await?;
    Ok(to_insert.len())
}

/// Verify that input frames have strictly increasing graph_ids and
/// non-decreasing timestamps.
fn validate_input_ordering(frames: &[Frame]) -> Result<()> {
    for i in 1..frames.len() {
        let prev = &frames[i - 1];
        let curr = &frames[i];

        anyhow::ensure!(
            curr.graph_id.0 > prev.graph_id.0,
            "input frames are not monotonically ordered: frame at index {i} \
             has graph_id={} which is not greater than previous graph_id={} \
             (at index {})",
            curr.graph_id.0,
            prev.graph_id.0,
            i - 1,
        );

        anyhow::ensure!(
            curr.timestamp >= prev.timestamp,
            "input frames are not monotonically ordered: frame at index {i} \
             has timestamp={} which is earlier than previous timestamp={} \
             (at index {}, graph_id={} vs graph_id={})",
            curr.timestamp,
            prev.timestamp,
            i - 1,
            curr.graph_id.0,
            prev.graph_id.0,
        );
    }
    Ok(())
}

/// Acquire an exclusive timeline lock and return the config.
async fn lock_timeline(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<TimelineConfig> {
    conn.get_timeline_config_and_lock(timeline_id, task)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "put_new_empty_frames: timeline '{}' not found",
                timeline_id.0,
            )
        })
}

/// Verify the timeline uses the AdjacentDeltas schema.
fn ensure_adjacent_deltas_schema(config: &TimelineConfig, timeline_id: &TimelineID) -> Result<()> {
    anyhow::ensure!(
        matches!(config.schema, TimelineSchema::AdjacentDeltas(_)),
        "put_new_empty_frames only supports AdjacentDeltas timelines, \
         but timeline '{}' uses {} schema",
        timeline_id.0,
        config.schema,
    );
    Ok(())
}

/// Load the last `STORED_FRAMES_LOOKBACK` stored frames (metadata only)
/// in ascending order. Only the tail is needed for overlap resolution.
async fn load_stored_tail(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<Vec<Frame>> {
    // Query in descending order with a limit, then reverse to get ascending.
    let mut rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                order: Some(Order::Desc),
                limit: Some(STORED_FRAMES_LOOKBACK),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
        .await?;

    rows.reverse();
    Ok(rows.into_iter().map(|r| r.frame).collect())
}

/// Determine which frames from `new_frames` need to be inserted by
/// resolving overlap with `stored_tail` (the last N stored frames).
///
/// The overlap must be a contiguous suffix of `stored_tail` that matches
/// a contiguous prefix of `new_frames` — both graph_id and timestamp must
/// agree for every overlapping pair.
///
/// When `require_overlap` is `true` and the timeline is non-empty, the
/// input must overlap with at least one stored frame.
///
/// Returns the slice of `new_frames` after the overlap (the frames to insert).
fn resolve_overlap<'a>(
    stored_tail: &[Frame],
    new_frames: &'a [Frame],
    require_overlap: bool,
) -> Result<&'a [Frame]> {
    if stored_tail.is_empty() {
        return Ok(new_frames);
    }

    let first_new = &new_frames[0];
    let last_stored = stored_tail.last().unwrap();

    // No overlap: new range starts after all stored frames.
    if first_new.graph_id.0 > last_stored.graph_id.0 {
        if require_overlap {
            anyhow::bail!(
                "require_overlap is set but no overlap found: the first input \
                 frame has graph_id={} which is after the last stored \
                 graph_id={}. The input must include at least one frame that \
                 already exists in storage to confirm continuity.",
                first_new.graph_id.0,
                last_stored.graph_id.0,
            );
        }
        return Ok(new_frames);
    }

    // Find where new_frames[0] appears in stored_tail (by graph_id).
    let overlap_start_in_stored = stored_tail
        .iter()
        .position(|f| f.graph_id == first_new.graph_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "overlap alignment failed: the first input frame has graph_id={}, \
                 which is less than or equal to the last stored graph_id={}, \
                 but graph_id={} does not exist in the last {} stored frames. \
                 Input frames must either start after all stored frames or \
                 overlap with a contiguous suffix of stored frames.",
                first_new.graph_id.0,
                last_stored.graph_id.0,
                first_new.graph_id.0,
                stored_tail.len(),
            )
        })?;

    // Walk the overlap: stored[overlap_start..] vs new[0..overlap_len].
    let stored_overlap = &stored_tail[overlap_start_in_stored..];
    let overlap_len = std::cmp::min(stored_overlap.len(), new_frames.len());

    for i in 0..overlap_len {
        let stored = &stored_overlap[i];
        let new = &new_frames[i];

        anyhow::ensure!(
            stored.graph_id == new.graph_id,
            "overlap alignment failed at overlap position {i}: \
             stored frame has graph_id={}, but input frame has graph_id={}. \
             The overlapping portion of input frames must exactly match \
             the tail of stored frames. Stored frames in overlap region: [{}]. \
             Input frames in overlap region: [{}].",
            stored.graph_id.0,
            new.graph_id.0,
            format_graph_ids(stored_overlap),
            format_graph_ids(&new_frames[..overlap_len]),
        );

        anyhow::ensure!(
            stored.timestamp == new.timestamp,
            "overlap alignment failed: frame with graph_id={} has \
             timestamp={} in input but timestamp={} in storage. \
             Timestamps must match for overlapping frames.",
            new.graph_id.0,
            new.timestamp,
            stored.timestamp,
        );
    }

    // If stored_overlap extends beyond new_frames, that means all input
    // frames are already stored — nothing to insert.
    if stored_overlap.len() >= new_frames.len() {
        return Ok(&new_frames[new_frames.len()..]);
    }

    Ok(&new_frames[overlap_len..])
}

/// Verify that the first frame to insert continues the monotonic sequence
/// after the last stored frame.
fn validate_monotonic_continuation(
    stored_tail: &[Frame],
    first_to_insert: &Frame,
    timeline_id: &TimelineID,
) -> Result<()> {
    let last_stored = match stored_tail.last() {
        Some(f) => f,
        None => return Ok(()),
    };

    anyhow::ensure!(
        first_to_insert.graph_id.0 > last_stored.graph_id.0,
        "monotonic ordering violated after overlap resolution in timeline '{}': \
         first frame to insert has graph_id={}, but the last stored frame \
         has graph_id={}. New frames must have strictly greater graph_id.",
        timeline_id.0,
        first_to_insert.graph_id.0,
        last_stored.graph_id.0,
    );

    anyhow::ensure!(
        first_to_insert.timestamp >= last_stored.timestamp,
        "monotonic ordering violated after overlap resolution in timeline '{}': \
         first frame to insert has timestamp={}, but the last stored frame \
         has timestamp={} (graph_id={} vs graph_id={}). \
         New frames must have non-decreasing timestamps.",
        timeline_id.0,
        first_to_insert.timestamp,
        last_stored.timestamp,
        first_to_insert.graph_id.0,
        last_stored.graph_id.0,
    );

    Ok(())
}

/// Insert empty frames in chunks within an existing transaction.
async fn insert_empty_frames_chunked(
    conn: &mut dyn UnigraphGraphConnection,
    timeline_id: &TimelineID,
    frames: &[Frame],
    task: &ll::Task,
) -> Result<()> {
    let total = frames.len() as i64;
    task.progress(0, total);

    for (i, chunk) in frames.chunks(EMPTY_FRAME_CHUNK_SIZE).enumerate() {
        for frame in chunk {
            let key = GraphTimeKey {
                timeline_id: timeline_id.clone(),
                timestamp: frame.timestamp,
                graph_id: frame.graph_id,
            };
            conn.store_frame_empty(&key, task).await?;
        }
        let done = ((i + 1) * EMPTY_FRAME_CHUNK_SIZE).min(frames.len()) as i64;
        task.progress(done, total);
    }
    Ok(())
}

/// Format a slice of frames as a comma-separated list of graph_ids for
/// error messages.
fn format_graph_ids(frames: &[Frame]) -> String {
    frames
        .iter()
        .map(|f| f.graph_id.0.to_string())
        .collect::<Vec<_>>()
        .join(", ")
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

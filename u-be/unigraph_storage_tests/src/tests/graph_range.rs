// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Tests for `GraphRangeBuilder` → `store_range` → `load_range` → `replay`
//! round-trip.
//!
//! Exercises the full CAS ingestion path: register Empty frames, build
//! graphs into a `GraphRangeBuilder`, store atomically via `store_range`,
//! then load back and verify each graph matches the original.

use std::sync::Arc;

use anyhow::Result;
use unigraph_db::GraphRangeBuilder;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;

use crate::*;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

async fn setup_timeline(db: &UnigraphDb, name: &str, task: &ll::Task) {
    db.timelines
        .create(
            &TimelineID(name.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: None,
            },
            task,
        )
        .await
        .unwrap();
}

/// Store a single range of graphs, load it back, verify round-trip.
#[tokio::test]
async fn store_and_load_single_range() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "test";
    setup_timeline(&db, tl, &task).await;

    let count = 20;
    let graphs: Vec<_> = (0..count).map(TestGraphTimeline::get_nth).collect();

    // Register Empty frames (Phase A of ingestion).
    let empty_frames: Vec<Frame> = (0..count)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i as i64),
            graph_id: GraphID(i as i64),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&TimelineID(tl.to_string()), empty_frames, false, &task)
        .await?;

    // Build a GraphRange (Phase B).
    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
    for i in 0..count {
        let key = make_graph_time_key(tl, i as i64, 1000 + i as i64);
        builder.add(key, TestGraphTimeline::get_nth(i))?;
    }
    let range = builder.finalize();

    // Store the range atomically (CAS).
    db.graph.adjacent_deltas.store_range(range, &task).await?;

    // Verify: fetch each graph individually and compare.
    for (i, graph) in graphs.iter().enumerate() {
        let fetched = db.graph.fetch(&make_graph_key(tl, i as i64), &task).await?;
        assert_graphs_equal(graph, &fetched);
    }

    // Verify: load the full range and replay.
    let loaded = db
        .graph
        .adjacent_deltas
        .load_range(&TimelineID(tl.to_string()), None, None, &task)
        .await?;
    assert_eq!(loaded.len(), count as usize);

    let mut replay_idx = 0usize;
    loaded.replay(|_key, graph| {
        assert_graphs_equal(&graphs[replay_idx], graph);
        replay_idx += 1;
        Ok(())
    })?;
    assert_eq!(replay_idx, count as usize);

    Ok(())
}

/// Store graphs across multiple ranges (simulating error-recovery flushes),
/// then load the entire timeline and verify.
#[tokio::test]
async fn store_multiple_ranges_then_load() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "multi";
    setup_timeline(&db, tl, &task).await;

    let total = 50u64;
    let graphs: Vec<_> = (0..total).map(TestGraphTimeline::get_nth).collect();

    // Register all Empty frames.
    let empty_frames: Vec<Frame> = (0..total)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i as i64),
            graph_id: GraphID(i as i64),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&TimelineID(tl.to_string()), empty_frames, false, &task)
        .await?;

    // Build and store in 3 separate ranges: [0..15), [15..35), [35..50).
    let splits = [0..15, 15..35, 35..50];
    for split in &splits {
        let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
        for i in split.clone() {
            let key = make_graph_time_key(tl, i as i64, 1000 + i as i64);
            builder.add(key, TestGraphTimeline::get_nth(i as u64))?;
        }
        let range = builder.finalize();
        db.graph.adjacent_deltas.store_range(range, &task).await?;
    }

    // Verify: fetch each graph individually.
    for (i, graph) in graphs.iter().enumerate() {
        let fetched = db.graph.fetch(&make_graph_key(tl, i as i64), &task).await?;
        assert_graphs_equal(graph, &fetched);
    }

    // Verify: load the full range and replay — each split starts with a Full,
    // so the loaded range will have multiple Full frames.
    let loaded = db
        .graph
        .adjacent_deltas
        .load_range(&TimelineID(tl.to_string()), None, None, &task)
        .await?;
    assert_eq!(loaded.len(), total as usize);

    let mut replay_idx = 0usize;
    loaded.replay(|_key, graph| {
        assert_graphs_equal(&graphs[replay_idx], graph);
        replay_idx += 1;
        Ok(())
    })?;
    assert_eq!(replay_idx, total as usize);

    Ok(())
}

/// Test the `take` (flush-on-error) pattern: build a range, flush partway,
/// store an error frame, continue with a fresh range.
#[tokio::test]
async fn flush_on_error_with_take() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "flush";
    setup_timeline(&db, tl, &task).await;

    let total = 10u64;
    let error_at = 5u64; // Simulate error at graph 5

    // Register all Empty frames.
    let empty_frames: Vec<Frame> = (0..total)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i as i64),
            graph_id: GraphID(i as i64),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&TimelineID(tl.to_string()), empty_frames, false, &task)
        .await?;

    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));

    for i in 0..total {
        let key = make_graph_time_key(tl, i as i64, 1000 + i as i64);

        if i == error_at {
            // Flush accumulated range before storing error.
            if !builder.is_empty() {
                let flushed = builder.take(TimelineID(tl.to_string()));
                db.graph.adjacent_deltas.store_range(flushed, &task).await?;
            }
            // Store error frame.
            db.graph
                .store_error(
                    &key,
                    &[TimestampedError {
                        timestamp: unigraph_timestamp::Timestamp::now(),
                        message: "simulated error".to_string(),
                    }],
                    &task,
                )
                .await?;
            continue;
        }

        builder.add(key, TestGraphTimeline::get_nth(i))?;
    }

    // Store remaining range.
    if !builder.is_empty() {
        let final_range = builder.finalize();
        db.graph
            .adjacent_deltas
            .store_range(final_range, &task)
            .await?;
    }

    // Verify: all non-error graphs are fetchable and correct.
    for i in 0..total {
        if i == error_at {
            // Error frame — fetch should fail.
            let result = db.graph.fetch(&make_graph_key(tl, i as i64), &task).await;
            assert!(result.is_err(), "expected error for graph_id={}", i);
            continue;
        }
        let fetched = db.graph.fetch(&make_graph_key(tl, i as i64), &task).await?;
        let expected = TestGraphTimeline::get_nth(i);
        assert_graphs_equal(&expected, &fetched);
    }

    // Verify frame types: 0-4 should be Full/Delta, 5 should be Error, 6-9 should be Full/Delta.
    let frames = db.frames.list(&TimelineID(tl.to_string()), &task).await?;
    assert_eq!(frames.len(), total as usize);
    assert_eq!(frames[5].frame_type, FrameType::Error);
    for i in [0, 1, 2, 3, 4, 6, 7, 8, 9] {
        assert!(
            frames[i].frame_type == FrameType::Full || frames[i].frame_type == FrameType::Delta,
            "frame {} should be Full or Delta, got {:?}",
            i,
            frames[i].frame_type
        );
    }

    Ok(())
}

/// A previously-failed frame is retried: `store_range` accepts the Error frame
/// as a CAS target and overwrites it, and the retry can carry its Empty
/// neighbour along in the same range.
#[tokio::test]
async fn retry_overwrites_error_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "retry";
    setup_timeline(&db, tl, &task).await;

    let empty_frames: Vec<Frame> = (0..3)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i),
            graph_id: GraphID(i),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&TimelineID(tl.to_string()), empty_frames, false, &task)
        .await?;

    // Frame 0 builds fine.
    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
    builder.add(
        make_graph_time_key(tl, 0, 1000),
        TestGraphTimeline::get_nth(0),
    )?;
    db.graph
        .adjacent_deltas
        .store_range(builder.finalize(), &task)
        .await?;

    // Frame 1 fails.
    db.graph
        .store_error(
            &make_graph_time_key(tl, 1, 1001),
            &[TimestampedError {
                timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(2000),
                message: "attempt 1 failed".to_string(),
            }],
            &task,
        )
        .await?;
    let frames = db.frames.list(&TimelineID(tl.to_string()), &task).await?;
    assert_eq!(frames[1].frame_type, FrameType::Error);

    // Retry frame 1 and pick up its still-Empty neighbour 2 in the same range.
    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
    builder.add(
        make_graph_time_key(tl, 1, 1001),
        TestGraphTimeline::get_nth(1),
    )?;
    builder.add(
        make_graph_time_key(tl, 2, 1002),
        TestGraphTimeline::get_nth(2),
    )?;
    db.graph
        .adjacent_deltas
        .store_range(builder.finalize(), &task)
        .await?;

    // The Error frame is gone and both graphs read back correctly.
    let frames = db.frames.list(&TimelineID(tl.to_string()), &task).await?;
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[1].frame_type, FrameType::Full);
    assert_eq!(frames[2].frame_type, FrameType::Delta);
    for i in 0..3 {
        let fetched = db.graph.fetch(&make_graph_key(tl, i), &task).await?;
        assert_graphs_equal(&TestGraphTimeline::get_nth(i as u64), &fetched);
    }

    Ok(())
}

/// Overwriting an Error frame hands its external blobs to the cleanup sweeper,
/// so a retry doesn't orphan the failed attempt's storage.
#[tokio::test]
async fn retry_registers_error_blobs_for_cleanup() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite);
    let task = ll::Task::create_new("test");
    let tl = "retryblobs";

    // External mode forces every blob out of the row and into blob storage.
    db.timelines
        .create(
            &TimelineID(tl.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            &task,
        )
        .await?;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(
            &TimelineID(tl.to_string()),
            vec![Frame {
                timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000),
                graph_id: GraphID(0),
            }],
            false,
            &task,
        )
        .await?;

    db.graph
        .store_error(
            &make_graph_time_key(tl, 0, 1000),
            &[TimestampedError {
                timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(2000),
                message: "attempt 1 failed".to_string(),
            }],
            &task,
        )
        .await?;

    // A successful store unregisters its own blobs, so nothing is pending yet.
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    assert!(
        pending.is_empty(),
        "store_error should leave nothing pending, got {:?}",
        pending,
    );

    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
    builder.add(
        make_graph_time_key(tl, 0, 1000),
        TestGraphTimeline::get_nth(0),
    )?;
    db.graph
        .adjacent_deltas
        .store_range(builder.finalize(), &task)
        .await?;

    // Exactly the overwritten Error frame's blobs are now awaiting the sweeper.
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    let prefix = format!("graphs/{}/0/", tl);
    assert!(
        pending.iter().any(|k| k.ends_with("_error_manifest.json")),
        "error manifest blob should be pending cleanup, got {:?}",
        pending,
    );
    assert!(
        pending.iter().all(|k| k.starts_with(&prefix)),
        "only frame 0's blobs should be pending cleanup, got {:?}",
        pending,
    );
    // The retry's own Full-frame blobs must NOT be queued for deletion.
    let manifest = format!("{}_manifest.json", prefix);
    assert!(
        !pending.contains(&manifest),
        "the new Full frame's manifest must stay live, got {:?}",
        pending,
    );

    Ok(())
}

/// CAS still refuses to clobber a frame that was already built successfully —
/// only Empty and Error are overwritable.
#[tokio::test]
async fn cas_rejects_already_built_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "built";
    setup_timeline(&db, tl, &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(
            &TimelineID(tl.to_string()),
            vec![Frame {
                timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000),
                graph_id: GraphID(0),
            }],
            false,
            &task,
        )
        .await?;

    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
    builder.add(
        make_graph_time_key(tl, 0, 1000),
        TestGraphTimeline::get_nth(0),
    )?;
    db.graph
        .adjacent_deltas
        .store_range(builder.finalize(), &task)
        .await?;

    let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
    builder.add(
        make_graph_time_key(tl, 0, 1000),
        TestGraphTimeline::get_nth(1),
    )?;
    let err = db
        .graph
        .adjacent_deltas
        .store_range(builder.finalize(), &task)
        .await
        .expect_err("storing over a Full frame should fail the CAS check");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("expected Empty or Error"),
        "unexpected error: {}",
        msg,
    );

    // The original graph is untouched.
    let fetched = db.graph.fetch(&make_graph_key(tl, 0), &task).await?;
    assert_graphs_equal(&TestGraphTimeline::get_nth(0), &fetched);

    Ok(())
}

/// Load a partial range (subrange of a timeline).
///
/// Stores graphs in multiple ranges so there are multiple Full frames,
/// then loads a subrange that starts at a Full frame boundary.
#[tokio::test]
async fn load_partial_range() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "partial";
    setup_timeline(&db, tl, &task).await;

    let total = 30u64;

    // Register all Empty frames.
    let empty_frames: Vec<Frame> = (0..total)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i as i64),
            graph_id: GraphID(i as i64),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&TimelineID(tl.to_string()), empty_frames, false, &task)
        .await?;

    // Store in 3 ranges: [0..10), [10..20), [20..30).
    // Each range starts with a Full frame.
    for chunk_start in [0, 10, 20] {
        let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
        for i in chunk_start..chunk_start + 10 {
            let key = make_graph_time_key(tl, i as i64, 1000 + i as i64);
            builder.add(key, TestGraphTimeline::get_nth(i))?;
        }
        db.graph
            .adjacent_deltas
            .store_range(builder.finalize(), &task)
            .await?;
    }

    // Load the middle range [10..19].
    let loaded = db
        .graph
        .adjacent_deltas
        .load_range(
            &TimelineID(tl.to_string()),
            Some(GraphID(10)),
            Some(GraphID(19)),
            &task,
        )
        .await?;
    assert_eq!(loaded.len(), 10);

    // Replay and verify each graph matches.
    let mut replay_idx = 10usize;
    loaded.replay(|key, graph| {
        assert_eq!(key.graph_id.0 as usize, replay_idx);
        let expected = TestGraphTimeline::get_nth(replay_idx as u64);
        assert_graphs_equal(&expected, graph);
        replay_idx += 1;
        Ok(())
    })?;
    assert_eq!(replay_idx, 20);

    Ok(())
}

/// Large randomized test: 200 graphs stored in multiple ranges.
#[tokio::test]
async fn randomized_multi_range_200() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = "rand200";
    setup_timeline(&db, tl, &task).await;

    let total = 200u64;
    let graphs: Vec<_> = (0..total).map(TestGraphTimeline::get_nth).collect();

    // Register all Empty frames.
    let empty_frames: Vec<Frame> = (0..total)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i as i64),
            graph_id: GraphID(i as i64),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&TimelineID(tl.to_string()), empty_frames, false, &task)
        .await?;

    // Store in chunks of varying sizes: 7, 13, 23, 11, ... (wrapping).
    let chunk_sizes = [7, 13, 23, 11, 17, 3, 29, 19, 5, 31];
    let mut offset = 0usize;
    let mut chunk_idx = 0;

    while offset < total as usize {
        let chunk_size = chunk_sizes[chunk_idx % chunk_sizes.len()];
        let end = (offset + chunk_size).min(total as usize);

        let mut builder = GraphRangeBuilder::new(TimelineID(tl.to_string()));
        for i in offset..end {
            let key = make_graph_time_key(tl, i as i64, 1000 + i as i64);
            builder.add(key, TestGraphTimeline::get_nth(i as u64))?;
        }
        db.graph
            .adjacent_deltas
            .store_range(builder.finalize(), &task)
            .await?;

        offset = end;
        chunk_idx += 1;
    }

    // Verify: fetch each graph individually.
    for (i, graph) in graphs.iter().enumerate() {
        let fetched = db.graph.fetch(&make_graph_key(tl, i as i64), &task).await?;
        assert_graphs_equal(graph, &fetched);
    }

    // Verify: load full range and replay.
    let loaded = db
        .graph
        .adjacent_deltas
        .load_range(&TimelineID(tl.to_string()), None, None, &task)
        .await?;
    assert_eq!(loaded.len(), total as usize);

    let mut replay_idx = 0usize;
    loaded.replay(|_key, graph| {
        assert_graphs_equal(&graphs[replay_idx], graph);
        replay_idx += 1;
        Ok(())
    })?;
    assert_eq!(replay_idx, total as usize);

    Ok(())
}

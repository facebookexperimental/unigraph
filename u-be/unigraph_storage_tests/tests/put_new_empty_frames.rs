// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Tests for `AdjacentDeltasOps::put_new_empty_frames`.
//!
//! Covers: fresh timeline, partial overlap, full overlap, concurrent writer
//! overlap, misaligned overlap, input ordering violations, chunked insertion,
//! and the `require_overlap` parameter.

use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
use unigraph_db::UnigraphDb;
use unigraph_error::format_for_user;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;

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

fn make_frame(graph_id: i64, seconds: i64) -> Frame {
    Frame {
        timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(seconds),
        graph_id: GraphID(graph_id),
    }
}

fn make_frames(range: std::ops::Range<i64>, base_ts: i64) -> Vec<Frame> {
    range.map(|i| make_frame(i, base_ts + i)).collect()
}

/// Fresh timeline with no stored frames — all input frames are inserted.
#[tokio::test]
async fn fresh_timeline_inserts_all() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    assert_eq!(inserted, 3);

    let stored = db.frames.list(&tl, &task).await?;
    snapshot!(
        format_frames_table(&stored),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
1                    1970-01-01T00:16:41.000Z Empty -
2                    1970-01-01T00:16:42.000Z Empty -
3                    1970-01-01T00:16:43.000Z Empty -
"
    );

    Ok(())
}

/// Empty input returns 0 without touching the database.
#[tokio::test]
async fn empty_input_is_noop() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, vec![], false, &task)
        .await?;

    assert_eq!(inserted, 0);
    Ok(())
}

/// Partial overlap: stored [1,2,3], input [3,4,5] → inserts [4,5].
#[tokio::test]
async fn partial_overlap_inserts_tail() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    // Pre-store frames 1, 2, 3
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    // Input overlaps at frame 3 and adds 4, 5
    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(3..6, 1000), false, &task)
        .await?;

    assert_eq!(inserted, 2);

    let stored = db.frames.list(&tl, &task).await?;
    assert_eq!(stored.len(), 5);
    assert_eq!(stored.last().unwrap().frame.graph_id, GraphID(5));

    Ok(())
}

/// Full overlap: all input frames already stored → inserts 0.
#[tokio::test]
async fn full_overlap_inserts_nothing() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..6, 1000), false, &task)
        .await?;

    // Input is [3,4,5] which are all already stored
    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(3..6, 1000), false, &task)
        .await?;

    assert_eq!(inserted, 0);

    let stored = db.frames.list(&tl, &task).await?;
    assert_eq!(stored.len(), 5);

    Ok(())
}

/// No overlap with require_overlap=false: input starts after all stored → pure append.
#[tokio::test]
async fn no_overlap_appends_all() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(4..6, 1000), false, &task)
        .await?;

    assert_eq!(inserted, 2);

    let stored = db.frames.list(&tl, &task).await?;
    assert_eq!(stored.len(), 5);

    Ok(())
}

/// No overlap with require_overlap=true: should fail.
#[tokio::test]
async fn no_overlap_with_require_overlap_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(4..6, 1000), true, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
require_overlap is set but no overlap found: the first input frame has graph_id=4 which is after the last stored graph_id=3. The input must include at least one frame that already exists in storage to confirm continuity.
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// require_overlap on an empty timeline is fine — no stored frames means no overlap needed.
#[tokio::test]
async fn require_overlap_on_empty_timeline_ok() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), true, &task)
        .await?;

    assert_eq!(inserted, 3);
    Ok(())
}

/// Concurrent writer scenario: stored [1,2,3], a concurrent writer added [4,5],
/// and our input is [3,4,5,6]. After the concurrent write, stored is [1,2,3,4,5].
/// We should filter out [3,4,5] and insert only [6].
#[tokio::test]
async fn concurrent_writer_larger_overlap() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    // Original stored: [1, 2, 3]
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    // Concurrent writer added [4, 5] (with overlap at 3)
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(3..6, 1000), false, &task)
        .await?;

    // Our input is [3, 4, 5, 6] — we expected overlap at 3 only,
    // but 4 and 5 are also already stored.
    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(3..7, 1000), false, &task)
        .await?;

    assert_eq!(inserted, 1);

    let stored = db.frames.list(&tl, &task).await?;
    assert_eq!(stored.len(), 6);
    assert_eq!(stored.last().unwrap().frame.graph_id, GraphID(6));

    Ok(())
}

/// Overlap misalignment on graph_id: stored has [1,2,3] but input starts
/// with graph_id=2 which matches stored, then jumps to graph_id=5 which
/// doesn't match stored[2]=3. This should fail.
#[tokio::test]
async fn overlap_misalignment_graph_id_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    // Input [2, 5] — graph_id=2 aligns with stored[1], but next should be 3, not 5
    let frames = vec![make_frame(2, 1002), make_frame(5, 1005)];

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
overlap alignment failed at overlap position 1: stored frame has graph_id=3, but input frame has graph_id=5. The overlapping portion of input frames must exactly match the tail of stored frames. Stored frames in overlap region: [2, 3]. Input frames in overlap region: [2, 5].
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// Overlap misalignment on timestamp: stored frame 3 has ts=1003 but
/// input frame 3 has ts=9999.
#[tokio::test]
async fn overlap_misalignment_timestamp_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..4, 1000), false, &task)
        .await?;

    // Input frame 3 has wrong timestamp
    let frames = vec![make_frame(3, 9999), make_frame(4, 10000)];

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
overlap alignment failed: frame with graph_id=3 has timestamp=1970-01-01 02:46:39 UTC in input but timestamp=1970-01-01 00:16:43 UTC in storage. Timestamps must match for overlapping frames.
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// Input with non-increasing graph_ids should fail validation.
#[tokio::test]
async fn input_ordering_violation_graph_id_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let frames = vec![make_frame(3, 1003), make_frame(2, 1002)];

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
input frames are not monotonically ordered: frame at index 1 has graph_id=2 which is not greater than previous graph_id=3 (at index 0)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// Input with decreasing timestamps should fail validation.
#[tokio::test]
async fn input_ordering_violation_timestamp_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let frames = vec![make_frame(1, 2000), make_frame(2, 1000)];

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
input frames are not monotonically ordered: frame at index 1 has timestamp=1970-01-01 00:16:40 UTC which is earlier than previous timestamp=1970-01-01 00:33:20 UTC (at index 0, graph_id=2 vs graph_id=1)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// Same timestamps are allowed (non-decreasing, not strictly increasing).
#[tokio::test]
async fn same_timestamps_are_allowed() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let frames = vec![
        make_frame(1, 1000),
        make_frame(2, 1000),
        make_frame(3, 1000),
    ];

    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await?;

    assert_eq!(inserted, 3);

    Ok(())
}

/// Input starts before stored frames (graph_id exists in stored but
/// isn't at the tail) — should fail.
#[tokio::test]
async fn input_starts_before_stored_tail_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(1..6, 1000), false, &task)
        .await?;

    // Input starts at graph_id=2 which exists in stored but the overlap
    // walks through stored[1..] = [2,3,4,5] vs input [2,3,10].
    // graph_id=10 doesn't match stored graph_id=4 → alignment error.
    let frames = vec![
        make_frame(2, 1002),
        make_frame(3, 1003),
        make_frame(10, 1010),
    ];

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
overlap alignment failed at overlap position 2: stored frame has graph_id=4, but input frame has graph_id=10. The overlapping portion of input frames must exactly match the tail of stored frames. Stored frames in overlap region: [2, 3, 4, 5]. Input frames in overlap region: [2, 3, 10].
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// Input graph_id is <= last stored but doesn't exist in stored frames.
#[tokio::test]
async fn input_graph_id_not_found_in_stored_fails() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    // Store frames 1, 3, 5 (gaps in graph_ids)
    let frames = vec![
        make_frame(1, 1001),
        make_frame(3, 1003),
        make_frame(5, 1005),
    ];
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await?;

    // Input starts at graph_id=4 which is <= 5 but doesn't exist
    let frames = vec![make_frame(4, 1004), make_frame(6, 1006)];

    let err = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await
        .unwrap_err();

    snapshot!(format_for_user(&err), "
overlap alignment failed: the first input frame has graph_id=4, which is less than or equal to the last stored graph_id=5, but graph_id=4 does not exist in the last 3 stored frames. Input frames must either start after all stored frames or overlap with a contiguous suffix of stored frames.
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: [Task] put_new_empty_frames
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

");

    Ok(())
}

/// Large batch to exercise chunked insertion.
#[tokio::test]
async fn large_batch_chunked_insertion() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let count = 25_000;
    let frames: Vec<Frame> = (0..count)
        .map(|i| make_frame(i as i64, 1000 + i as i64))
        .collect();

    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, frames, false, &task)
        .await?;

    assert_eq!(inserted, count);

    let stored = db.frames.list(&tl, &task).await?;
    assert_eq!(stored.len(), count);

    Ok(())
}

/// Stored frames followed by put_new_empty_frames, then CAS store_range
/// works end-to-end.
#[tokio::test]
async fn put_empty_then_store_range_e2e() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let tl_str = "test";
    let tl = TimelineID(tl_str.to_string());
    setup_timeline(&db, tl_str, &task).await;

    // Register empty frames via put_new_empty_frames
    let inserted = db
        .graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, make_frames(0..3, 1000), false, &task)
        .await?;
    assert_eq!(inserted, 3);

    // Build graphs and store via CAS
    let mut builder = unigraph_db::GraphRangeBuilder::new(tl.clone());
    for i in 0..3 {
        let key = make_graph_time_key(tl_str, i, 1000 + i);
        builder.add(key, TestGraphTimeline::get_nth(i as u64))?;
    }
    let range = builder.finalize();
    db.graph.adjacent_deltas.store_range(range, &task).await?;

    // Verify each graph can be fetched
    for i in 0..3 {
        let expected = TestGraphTimeline::get_nth(i as u64);
        let fetched = db.graph.fetch(&make_graph_key(tl_str, i), &task).await?;
        assert_graphs_equal(&expected, &fetched);
    }

    Ok(())
}

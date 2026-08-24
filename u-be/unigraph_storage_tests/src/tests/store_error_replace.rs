// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Storing an Error frame over whatever is already at the key.
//!
//! The ingestion jobs' whole retry policy rests on this: each failure is stored
//! as an Error frame carrying every attempt so far, so the stored list's length
//! *is* the attempt count and `--max-attempts` is what retires a frame that
//! cannot be built. If the second failure cannot be written, the count is
//! pinned at one, the cap is unreachable, and the frame is retried forever —
//! which is what `www` and `www-budget` were doing to 2,150 frames.
//!
//! The other half is that a stale failure must never clobber a graph that some
//! other worker built in the meantime.

use std::sync::Arc;

use anyhow::Result;
use unigraph_db::ErrorFrameStored;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;

use crate::*;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

async fn setup_timeline(db: &UnigraphDb, name: &str, task: &ll::Task) -> Result<TimelineID> {
    let timeline_id = TimelineID(name.to_string());
    db.timelines
        .create(
            &timeline_id,
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: None,
            },
            task,
        )
        .await?;
    Ok(timeline_id)
}

fn attempt(n: i64) -> TimestampedError {
    TimestampedError {
        timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + n),
        message: format!("attempt {n} failed"),
    }
}

// ── Tests ────────────────────────────────────────────────────

/// Every failure lands, so the attempt count actually climbs and
/// `--max-attempts` can be reached.
#[tokio::test]
async fn each_failure_extends_the_attempt_count() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "test", &task).await?;
    let key = make_graph_time_key("test", 1, 1000);

    // Three failures in a row, each re-reading and appending the way the
    // ingestion jobs do.
    for n in 1..=3 {
        let so_far: Vec<TimestampedError> = (1..=n).map(attempt).collect();
        assert_eq!(
            db.graph.store_error(&key, &so_far, &task).await?,
            ErrorFrameStored::Stored,
            "attempt {n} should have been recorded"
        );
        assert_eq!(
            db.graph.fetch_errors(&key.graph_key(), &task).await?.len(),
            n as usize,
            "the stored list length is the attempt count"
        );
    }

    let frames = db.frames.list(&timeline_id, &task).await?;
    assert_eq!(frames.len(), 1, "replacement must not leave extra rows");
    assert_eq!(frames[0].frame_type, FrameType::Error);

    Ok(())
}

/// A failure that arrives after someone else built the frame is discarded, not
/// written over the top of a good graph.
///
/// Both writers take the timeline lock (`SELECT ... FOR UPDATE` on MySQL,
/// `BEGIN EXCLUSIVE` on SQLite) before touching a frame, so the re-read inside
/// `store_error` cannot be raced — by the time it sees `Full`, the builder has
/// committed.
#[tokio::test]
async fn a_stale_failure_does_not_clobber_a_built_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "test", &task).await?;
    let key = make_graph_time_key("test", 1, 1000);
    let graph = TestGraphTimeline::get_nth(1);

    // This worker fails once and records it.
    db.graph.store_error(&key, &[attempt(1)], &task).await?;

    // Another worker succeeds and stores the graph.
    db.graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;
    db.graph.store(&key, &graph, None, &task).await?;

    // The first worker's *second* failure arrives late.
    let outcome = db
        .graph
        .store_error(&key, &[attempt(1), attempt(2)], &task)
        .await?;
    assert_eq!(
        outcome,
        ErrorFrameStored::SupersededBy(FrameType::Full),
        "a built frame must supersede the failure, not be overwritten by it"
    );

    // The graph is untouched and still readable.
    let frames = db.frames.list(&timeline_id, &task).await?;
    assert_eq!(frames[0].frame_type, FrameType::Full);
    assert_graphs_equal(&graph, &db.graph.fetch(&key.graph_key(), &task).await?);

    Ok(())
}

/// The first failure on a never-built frame replaces its Empty placeholder.
#[tokio::test]
async fn the_first_failure_replaces_the_empty_placeholder() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "test", &task).await?;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(
            &timeline_id,
            vec![Frame {
                timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000),
                graph_id: GraphID(1),
            }],
            false,
            &task,
        )
        .await?;

    let key = make_graph_time_key("test", 1, 1000);
    assert_eq!(
        db.graph.store_error(&key, &[attempt(1)], &task).await?,
        ErrorFrameStored::Stored
    );

    let frames = db.frames.list(&timeline_id, &task).await?;
    assert_eq!(frames.len(), 1, "the placeholder should be gone, not kept");
    assert_eq!(frames[0].frame_type, FrameType::Error);

    Ok(())
}

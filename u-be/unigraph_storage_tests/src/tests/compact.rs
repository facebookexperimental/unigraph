// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
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

#[tokio::test]
async fn compact_converts_full_to_delta() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    // Store 5 Full frames
    let graphs: Vec<_> = (0..5).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..5)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        db.graph.store(key, &graphs[i], None, &task).await?;
    }

    // Before compaction: all Full
    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Full -          -
2                    1970-01-01T00:16:42.000Z Full -          -
3                    1970-01-01T00:16:43.000Z Full -          -
4                    1970-01-01T00:16:44.000Z Full -          -
"
    );

    // Compact
    let converted = db
        .graph
        .compact(&TimelineID("test".to_string()), None, None, &task)
        .await?;
    assert_eq!(converted, 4);

    // After compaction: 1 Full + 4 Delta
    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Delta test:0     -
2                    1970-01-01T00:16:42.000Z Delta test:1     -
3                    1970-01-01T00:16:43.000Z Delta test:2     -
4                    1970-01-01T00:16:44.000Z Delta test:3     -
"
    );

    // All graphs should still be fetchable
    for (i, key) in keys.iter().enumerate() {
        let fetched = db.graph.fetch(&key.graph_key(), &task).await?;
        assert_graphs_equal(&graphs[i], &fetched);
    }

    // Idempotent: compacting again should convert 0 frames
    let converted = db
        .graph
        .compact(&TimelineID("test".to_string()), None, None, &task)
        .await?;
    assert_eq!(converted, 0);

    Ok(())
}

#[tokio::test]
async fn compact_with_error_gap() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    // Full, Full, Error, Full, Full
    let graphs: Vec<_> = (0..4).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..5)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    db.graph.store(&keys[0], &graphs[0], None, &task).await?;
    db.graph.store(&keys[1], &graphs[1], None, &task).await?;
    db.graph
        .store_error(
            &keys[2],
            &[TimestampedError {
                timestamp: keys[2].timestamp,
                message: "test error".to_string(),
            }],
            &task,
        )
        .await?;
    db.graph.store(&keys[3], &graphs[2], None, &task).await?;
    db.graph.store(&keys[4], &graphs[3], None, &task).await?;

    let converted = db
        .graph
        .compact(&TimelineID("test".to_string()), None, None, &task)
        .await?;
    assert_eq!(converted, 2); // keys[1] and keys[4]

    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Delta test:0     -
2                    1970-01-01T00:16:42.000Z Error -          -
3                    1970-01-01T00:16:43.000Z Full -          -
4                    1970-01-01T00:16:44.000Z Delta test:3     -
"
    );

    // Both chains should be fetchable
    let fetched_0 = db.graph.fetch(&keys[0].graph_key(), &task).await?;
    assert_graphs_equal(&graphs[0], &fetched_0);

    let fetched_1 = db.graph.fetch(&keys[1].graph_key(), &task).await?;
    assert_graphs_equal(&graphs[1], &fetched_1);

    let fetched_3 = db.graph.fetch(&keys[3].graph_key(), &task).await?;
    assert_graphs_equal(&graphs[2], &fetched_3);

    let fetched_4 = db.graph.fetch(&keys[4].graph_key(), &task).await?;
    assert_graphs_equal(&graphs[3], &fetched_4);

    Ok(())
}

#[tokio::test]
async fn compact_already_compact() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    // Store 3 Full frames, compact once, then verify second compact is a no-op
    let graphs: Vec<_> = (0..3).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..3)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        db.graph.store(key, &graphs[i], None, &task).await?;
    }

    // First compact: converts 2 Full → Delta
    let converted = db
        .graph
        .compact(&TimelineID("test".to_string()), None, None, &task)
        .await?;
    assert_eq!(converted, 2);

    // Second compact: already compact, should be a no-op
    let converted = db
        .graph
        .compact(&TimelineID("test".to_string()), None, None, &task)
        .await?;
    assert_eq!(converted, 0);

    // Still all fetchable
    for (i, key) in keys.iter().enumerate() {
        let fetched = db.graph.fetch(&key.graph_key(), &task).await?;
        assert_graphs_equal(&graphs[i], &fetched);
    }

    Ok(())
}

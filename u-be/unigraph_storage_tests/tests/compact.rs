// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

async fn setup_timeline(db: &UnigraphDb, name: &str) {
    db.create_timeline(
        &TimelineID(name.to_string()),
        &TimelineConfig {
            schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            external_id_namespace: None,
            blob_storage: Default::default(),
            store_metric_history: None,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn compact_converts_full_to_delta() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    // Store 5 Full frames
    let graphs: Vec<_> = (0..5).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..5)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        db.store_graph_full(key, &graphs[i]).await?;
    }

    // Before compaction: all Full
    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -
1                    1970-01-01T00:16:41.000Z Full -
2                    1970-01-01T00:16:42.000Z Full -
3                    1970-01-01T00:16:43.000Z Full -
4                    1970-01-01T00:16:44.000Z Full -
"
    );

    // Compact
    let converted = db
        .compact_timeline(&TimelineID("test".to_string()), None, None)
        .await?;
    assert_eq!(converted, 4);

    // After compaction: 1 Full + 4 Delta
    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -
1                    1970-01-01T00:16:41.000Z Delta test:0
2                    1970-01-01T00:16:42.000Z Delta test:1
3                    1970-01-01T00:16:43.000Z Delta test:2
4                    1970-01-01T00:16:44.000Z Delta test:3
"
    );

    // All graphs should still be fetchable
    for (i, key) in keys.iter().enumerate() {
        let fetched = db.fetch_graph(&key.graph_key()).await?;
        assert_graphs_equal(&graphs[i], &fetched);
    }

    // Idempotent: compacting again should convert 0 frames
    let converted = db
        .compact_timeline(&TimelineID("test".to_string()), None, None)
        .await?;
    assert_eq!(converted, 0);

    Ok(())
}

#[tokio::test]
async fn compact_with_error_gap() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    // Full, Full, Error, Full, Full
    let graphs: Vec<_> = (0..4).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..5)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    db.store_graph_full(&keys[0], &graphs[0]).await?;
    db.store_graph_full(&keys[1], &graphs[1]).await?;
    db.store_error(
        &keys[2],
        &[TimestampedError {
            timestamp: keys[2].timestamp,
            message: "test error".to_string(),
        }],
    )
    .await?;
    db.store_graph_full(&keys[3], &graphs[2]).await?;
    db.store_graph_full(&keys[4], &graphs[3]).await?;

    let converted = db
        .compact_timeline(&TimelineID("test".to_string()), None, None)
        .await?;
    assert_eq!(converted, 2); // keys[1] and keys[4]

    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -
1                    1970-01-01T00:16:41.000Z Delta test:0
2                    1970-01-01T00:16:42.000Z Error -
3                    1970-01-01T00:16:43.000Z Full -
4                    1970-01-01T00:16:44.000Z Delta test:3
"
    );

    // Both chains should be fetchable
    let fetched_0 = db.fetch_graph(&keys[0].graph_key()).await?;
    assert_graphs_equal(&graphs[0], &fetched_0);

    let fetched_1 = db.fetch_graph(&keys[1].graph_key()).await?;
    assert_graphs_equal(&graphs[1], &fetched_1);

    let fetched_3 = db.fetch_graph(&keys[3].graph_key()).await?;
    assert_graphs_equal(&graphs[2], &fetched_3);

    let fetched_4 = db.fetch_graph(&keys[4].graph_key()).await?;
    assert_graphs_equal(&graphs[3], &fetched_4);

    Ok(())
}

#[tokio::test]
async fn compact_already_compact() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    // Store Full ← Delta ← Delta (already compact)
    let graphs: Vec<_> = (0..3).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..3)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    db.store_graph_full(&keys[0], &graphs[0]).await?;
    db.store_graph_delta(&keys[1], &keys[0].graph_key(), &graphs[1])
        .await?;
    db.store_graph_delta(&keys[2], &keys[1].graph_key(), &graphs[2])
        .await?;

    let converted = db
        .compact_timeline(&TimelineID("test".to_string()), None, None)
        .await?;
    assert_eq!(converted, 0);

    // Still all fetchable
    for (i, key) in keys.iter().enumerate() {
        let fetched = db.fetch_graph(&key.graph_key()).await?;
        assert_graphs_equal(&graphs[i], &fetched);
    }

    Ok(())
}

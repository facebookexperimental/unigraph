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
async fn store_full_then_delta_and_fetch() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let graph_0 = TestGraphTimeline::get_nth(0);
    let graph_1 = TestGraphTimeline::get_nth(1);

    let key_0 = make_graph_time_key("test", 0, 1000);
    let key_1 = make_graph_time_key("test", 1, 1001);

    // Store full graph
    db.store_graph_full(&key_0, &graph_0).await?;

    // Store delta
    db.store_graph_delta(&key_1, &key_0.graph_key(), &graph_1)
        .await?;

    // Verify listing
    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -
1                    1970-01-01T00:16:41.000Z Delta test:0
"
    );

    // Fetch the delta — should reconstruct graph_1
    let fetched = db.fetch_graph(&key_1.graph_key()).await?;
    assert_graphs_equal(&graph_1, &fetched);

    Ok(())
}

#[tokio::test]
async fn delta_chain_full_d_d_d() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    // Full ← Delta ← Delta ← Delta
    let graphs: Vec<_> = (0..4).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..4)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    // Store Full
    db.store_graph_full(&keys[0], &graphs[0]).await?;

    // Store chain of deltas
    for i in 1..4 {
        db.store_graph_delta(&keys[i], &keys[i - 1].graph_key(), &graphs[i])
            .await?;
    }

    // Verify listing
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
"
    );

    // Fetch the last delta — should recursively resolve the whole chain
    let fetched = db.fetch_graph(&keys[3].graph_key()).await?;
    assert_graphs_equal(&graphs[3], &fetched);

    // Also verify intermediate fetches
    let fetched_1 = db.fetch_graph(&keys[1].graph_key()).await?;
    assert_graphs_equal(&graphs[1], &fetched_1);

    let fetched_2 = db.fetch_graph(&keys[2].graph_key()).await?;
    assert_graphs_equal(&graphs[2], &fetched_2);

    Ok(())
}

#[tokio::test]
async fn delta_chain_with_intermediate_full() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    // Full ← Delta ← Full ← Delta
    let graphs: Vec<_> = (10..14).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..4)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    db.store_graph_full(&keys[0], &graphs[0]).await?;
    db.store_graph_delta(&keys[1], &keys[0].graph_key(), &graphs[1])
        .await?;
    db.store_graph_full(&keys[2], &graphs[2]).await?;
    db.store_graph_delta(&keys[3], &keys[2].graph_key(), &graphs[3])
        .await?;

    // Fetch from different points
    let fetched_0 = db.fetch_graph(&keys[0].graph_key()).await?;
    assert_graphs_equal(&graphs[0], &fetched_0);

    let fetched_1 = db.fetch_graph(&keys[1].graph_key()).await?;
    assert_graphs_equal(&graphs[1], &fetched_1);

    let fetched_2 = db.fetch_graph(&keys[2].graph_key()).await?;
    assert_graphs_equal(&graphs[2], &fetched_2);

    let fetched_3 = db.fetch_graph(&keys[3].graph_key()).await?;
    assert_graphs_equal(&graphs[3], &fetched_3);

    Ok(())
}

#[tokio::test]
async fn cross_timeline_delta_reference_rejected() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "timeline_a").await;
    setup_timeline(&db, "timeline_b").await;

    let graph_a = TestGraphTimeline::get_nth(100);
    let graph_b = TestGraphTimeline::get_nth(101);

    let key_a = make_graph_time_key("timeline_a", 100, 1000);
    let key_b = make_graph_time_key("timeline_b", 101, 2000);

    // Store full in timeline_a
    db.store_graph_full(&key_a, &graph_a).await?;

    // Store delta in timeline_b referencing timeline_a — should fail
    // because AdjacentDeltas requires the base to be the preceding frame
    // in the same timeline.
    let result = db
        .store_graph_delta(&key_b, &key_a.graph_key(), &graph_b)
        .await;
    assert!(
        result.is_err(),
        "cross-timeline delta reference should be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn get_preceding_frame() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let keys: Vec<_> = (0..3)
        .map(|i| make_graph_time_key("test", i as i64, 1000 + i as i64))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        let graph = TestGraphTimeline::get_nth(i as u64);
        db.store_graph_full(key, &graph).await?;
    }

    // Preceding frame of g_2 should be g_1
    let preceding = db
        .get_preceding_frame(&keys[2])
        .await?
        .expect("Should have preceding frame");
    assert_eq!(preceding.frame.graph_id.0, 1);

    // Preceding frame of g_0 should be None
    let preceding = db.get_preceding_frame(&keys[0]).await?;
    assert!(preceding.is_none());

    Ok(())
}

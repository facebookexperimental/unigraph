// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

async fn setup_timeline(db: &UnigraphDb, name: &str) {
    db.timelines
        .create(
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

async fn setup_timeline_full_or_delta(db: &UnigraphDb, name: &str) {
    db.timelines
        .create(
            &TimelineID(name.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::FullOrDelta(FullOrDeltaConfig {}),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: None,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn randomized_full_graph_roundtrip_1000() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    // Generate 1000 graphs
    let graphs: Vec<_> = (0..1000)
        .map(|i| (i, TestGraphTimeline::get_nth(i)))
        .collect();

    // Store in monotonic order (required by AdjacentDeltas invariant)
    for &(i, ref graph) in &graphs {
        let key = make_graph_time_key("test", i as i64, 1000 + i as i64);
        db.graph.store(&key, graph).await?;
    }

    // Verify all 1000 graphs round-trip correctly
    for (i, graph) in &graphs {
        let fetched = db.graph.fetch(&make_graph_key("test", *i as i64)).await?;
        assert_graphs_equal(graph, &fetched);
    }

    Ok(())
}

#[tokio::test]
async fn randomized_delta_chain_100() -> Result<()> {
    let db = make_db();
    setup_timeline_full_or_delta(&db, "test").await;

    let count = 100;
    let graphs: Vec<_> = (0..count).map(TestGraphTimeline::get_nth).collect();

    // Store first as full
    let key_0 = make_graph_time_key("test", 0, 1000);
    db.graph.store(&key_0, &graphs[0]).await?;

    // Store the rest as sequential deltas
    for (i, graph) in graphs.iter().enumerate().skip(1) {
        let key = make_graph_time_key("test", i as i64, 1000 + i as i64);
        let base_key = make_graph_key("test", (i - 1) as i64);
        db.graph.store_as_delta_from(&key, graph, &base_key).await?;
    }

    // Verify all 100 graphs round-trip correctly
    for (i, graph) in graphs.iter().enumerate() {
        let fetched = db.graph.fetch(&make_graph_key("test", i as i64)).await?;
        assert_graphs_equal(graph, &fetched);
    }

    Ok(())
}

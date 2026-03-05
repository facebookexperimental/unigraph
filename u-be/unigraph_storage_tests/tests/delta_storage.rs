// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;

fn make_storage() -> UnigraphStorage {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphStorage::new(sqlite.clone(), sqlite)
}

fn setup_timeline(storage: &UnigraphStorage, name: &str) {
    storage
        .graph
        .create_timeline(
            &TimelineID(name.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            },
        )
        .unwrap();
}

#[test]
fn store_full_then_delta_and_fetch() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    let graph_0 = TestGraphTimeline::get_nth(0);
    let graph_1 = TestGraphTimeline::get_nth(1);

    let key_0 = make_graph_time_key("test", "g_0", 1000);
    let key_1 = make_graph_time_key("test", "g_1", 1001);

    // Store full graph
    storage.store_graph_full(&key_0, &graph_0)?;

    // Store delta
    storage.store_graph_delta(&key_1, &key_0.graph_key(), &graph_1)?;

    // Verify listing
    let frames = storage.graph.list_frames(&TimelineID("test".to_string()))?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base      
----------------------------------------------------------------------
g_0                  1970-01-01T00:16:40Z     Full       -         
g_1                  1970-01-01T00:16:41Z     Delta      test:g_0  
"
    );

    // Fetch the delta — should reconstruct graph_1
    let fetched = storage.fetch_graph(&key_1.graph_key())?;
    assert_graphs_equal(&graph_1, &fetched);

    Ok(())
}

#[test]
fn delta_chain_full_d_d_d() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    // Full ← Delta ← Delta ← Delta
    let graphs: Vec<_> = (0..4).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..4)
        .map(|i| make_graph_time_key("test", &format!("g_{}", i), 1000 + i as i64))
        .collect();

    // Store Full
    storage.store_graph_full(&keys[0], &graphs[0])?;

    // Store chain of deltas
    for i in 1..4 {
        storage.store_graph_delta(&keys[i], &keys[i - 1].graph_key(), &graphs[i])?;
    }

    // Verify listing
    let frames = storage.graph.list_frames(&TimelineID("test".to_string()))?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base      
----------------------------------------------------------------------
g_0                  1970-01-01T00:16:40Z     Full       -         
g_1                  1970-01-01T00:16:41Z     Delta      test:g_0  
g_2                  1970-01-01T00:16:42Z     Delta      test:g_1  
g_3                  1970-01-01T00:16:43Z     Delta      test:g_2  
"
    );

    // Fetch the last delta — should recursively resolve the whole chain
    let fetched = storage.fetch_graph(&keys[3].graph_key())?;
    assert_graphs_equal(&graphs[3], &fetched);

    // Also verify intermediate fetches
    let fetched_1 = storage.fetch_graph(&keys[1].graph_key())?;
    assert_graphs_equal(&graphs[1], &fetched_1);

    let fetched_2 = storage.fetch_graph(&keys[2].graph_key())?;
    assert_graphs_equal(&graphs[2], &fetched_2);

    Ok(())
}

#[test]
fn delta_chain_with_intermediate_full() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    // Full ← Delta ← Full ← Delta
    let graphs: Vec<_> = (10..14).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..4)
        .map(|i| make_graph_time_key("test", &format!("g_{}", i), 1000 + i as i64))
        .collect();

    storage.store_graph_full(&keys[0], &graphs[0])?;
    storage.store_graph_delta(&keys[1], &keys[0].graph_key(), &graphs[1])?;
    storage.store_graph_full(&keys[2], &graphs[2])?;
    storage.store_graph_delta(&keys[3], &keys[2].graph_key(), &graphs[3])?;

    // Fetch from different points
    let fetched_0 = storage.fetch_graph(&keys[0].graph_key())?;
    assert_graphs_equal(&graphs[0], &fetched_0);

    let fetched_1 = storage.fetch_graph(&keys[1].graph_key())?;
    assert_graphs_equal(&graphs[1], &fetched_1);

    let fetched_2 = storage.fetch_graph(&keys[2].graph_key())?;
    assert_graphs_equal(&graphs[2], &fetched_2);

    let fetched_3 = storage.fetch_graph(&keys[3].graph_key())?;
    assert_graphs_equal(&graphs[3], &fetched_3);

    Ok(())
}

#[test]
fn cross_timeline_delta_reference() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "timeline_a");
    setup_timeline(&storage, "timeline_b");

    let graph_a = TestGraphTimeline::get_nth(100);
    let graph_b = TestGraphTimeline::get_nth(101);

    let key_a = make_graph_time_key("timeline_a", "g_a", 1000);
    let key_b = make_graph_time_key("timeline_b", "g_b", 2000);

    // Store full in timeline_a
    storage.store_graph_full(&key_a, &graph_a)?;

    // Store delta in timeline_b referencing timeline_a
    storage.store_graph_delta(&key_b, &key_a.graph_key(), &graph_b)?;

    // Fetch from timeline_b — should resolve cross-timeline
    let fetched = storage.fetch_graph(&key_b.graph_key())?;
    assert_graphs_equal(&graph_b, &fetched);

    Ok(())
}

#[test]
fn get_preceding_frame() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    let keys: Vec<_> = (0..3)
        .map(|i| make_graph_time_key("test", &format!("g_{}", i), 1000 + i as i64))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        let graph = TestGraphTimeline::get_nth(i as u64);
        storage.store_graph_full(key, &graph)?;
    }

    // Preceding frame of g_2 should be g_1
    let preceding = storage
        .graph
        .get_preceding_frame(&keys[2])?
        .expect("Should have preceding frame");
    assert_eq!(preceding.frame.graph_id.0, "g_1");

    // Preceding frame of g_0 should be None
    let preceding = storage.graph.get_preceding_frame(&keys[0])?;
    assert!(preceding.is_none());

    Ok(())
}

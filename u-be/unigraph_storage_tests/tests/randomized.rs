// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;
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
fn randomized_full_graph_roundtrip_1000() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    // Generate 1000 graphs
    let graphs: Vec<_> = (0..1000)
        .map(|i| (i, TestGraphTimeline::get_nth(i)))
        .collect();

    // Store in shuffled order (Fisher-Yates using simple XOR-shift)
    let mut indices: Vec<u64> = (0..1000).collect();
    let mut rng_state: u64 = 12345;
    for i in (1..indices.len()).rev() {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let j = (rng_state as usize) % (i + 1);
        indices.swap(i, j);
    }

    for &i in &indices {
        let key = make_graph_time_key("test", &format!("g_{:04}", i), 1000 + i as i64);
        storage.store_graph_full(&key, &graphs[i as usize].1)?;
    }

    // Verify all 1000 graphs round-trip correctly
    for (i, graph) in &graphs {
        let fetched = storage.fetch_graph(&make_graph_key("test", &format!("g_{:04}", i)))?;
        assert_graphs_equal(graph, &fetched);
    }

    Ok(())
}

#[test]
fn randomized_delta_chain_100() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    let count = 100;
    let graphs: Vec<_> = (0..count).map(TestGraphTimeline::get_nth).collect();

    // Store first as full
    let key_0 = make_graph_time_key("test", "g_0000", 1000);
    storage.store_graph_full(&key_0, &graphs[0])?;

    // Store the rest as sequential deltas
    for (i, graph) in graphs.iter().enumerate().skip(1) {
        let key = make_graph_time_key("test", &format!("g_{:04}", i), 1000 + i as i64);
        let base_key = make_graph_key("test", &format!("g_{:04}", i - 1));
        storage.store_graph_delta(&key, &base_key, graph)?;
    }

    // Verify all 100 graphs round-trip correctly
    for (i, graph) in graphs.iter().enumerate() {
        let fetched = storage.fetch_graph(&make_graph_key("test", &format!("g_{:04}", i)))?;
        assert_graphs_equal(graph, &fetched);
    }

    Ok(())
}

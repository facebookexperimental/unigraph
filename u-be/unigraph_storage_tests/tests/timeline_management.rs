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

#[test]
fn create_and_get_timeline_config() -> Result<()> {
    let storage = make_storage();

    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
    };

    storage
        .graph
        .create_timeline(&TimelineID("my_timeline".to_string()), &config)?;

    let fetched = storage
        .graph
        .get_timeline_config(&TimelineID("my_timeline".to_string()))?
        .expect("Timeline should exist");

    // Verify the schema type survived round-trip
    match fetched.schema {
        TimelineSchema::AdjacentDeltas(_) => {} // expected
    }

    Ok(())
}

#[test]
fn get_nonexistent_timeline_returns_none() -> Result<()> {
    let storage = make_storage();

    let result = storage
        .graph
        .get_timeline_config(&TimelineID("nonexistent".to_string()))?;
    assert!(result.is_none());

    Ok(())
}

#[test]
fn list_timelines() -> Result<()> {
    let storage = make_storage();

    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
    };

    storage
        .graph
        .create_timeline(&TimelineID("beta".to_string()), &config)?;
    storage
        .graph
        .create_timeline(&TimelineID("alpha".to_string()), &config)?;
    storage
        .graph
        .create_timeline(&TimelineID("gamma".to_string()), &config)?;

    let timelines = storage.graph.list_timelines()?;
    let names: Vec<_> = timelines.iter().map(|t| t.0.as_str()).collect();

    // Should be sorted
    assert_eq!(names, vec!["alpha", "beta", "gamma"]);

    Ok(())
}

#[test]
fn frames_ordered_by_timestamp_then_graph_id() -> Result<()> {
    use chrono::TimeZone;

    let storage = make_storage();
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
    };
    storage
        .graph
        .create_timeline(&TimelineID("test".to_string()), &config)?;

    // Insert frames at the same timestamp with different graph IDs
    let ts = chrono::Utc.timestamp_opt(1000, 0).unwrap();

    for id in ["c", "a", "b"] {
        let graph = TestGraphTimeline::get_nth(id.as_bytes()[0] as u64);
        let key = GraphTimeKey {
            timeline_id: TimelineID("test".to_string()),
            timestamp: ts,
            graph_id: GraphID(id.to_string()),
        };
        storage.store_graph_full(&key, &graph)?;
    }

    let frames = storage.graph.list_frames(&TimelineID("test".to_string()))?;
    let ids: Vec<_> = frames.iter().map(|f| f.frame.graph_id.0.as_str()).collect();

    // Same timestamp → ordered by graph_id
    assert_eq!(ids, vec!["a", "b", "c"]);

    Ok(())
}

#[test]
fn list_frames_range() -> Result<()> {
    use chrono::TimeZone;

    let storage = make_storage();
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
    };
    storage
        .graph
        .create_timeline(&TimelineID("test".to_string()), &config)?;

    for i in 0..10 {
        let graph = TestGraphTimeline::get_nth(i);
        let key = make_graph_time_key("test", &format!("g_{}", i), 1000 + i as i64);
        storage.store_graph_full(&key, &graph)?;
    }

    // Query a range that should include frames 3-7
    let start = chrono::Utc.timestamp_opt(1003, 0).unwrap();
    let end = chrono::Utc.timestamp_opt(1007, 0).unwrap();

    let frames = storage
        .graph
        .list_frames_range(&TimelineID("test".to_string()), start, end)?;

    let ids: Vec<_> = frames.iter().map(|f| f.frame.graph_id.0.as_str()).collect();
    assert_eq!(ids, vec!["g_3", "g_4", "g_5", "g_6", "g_7"]);

    Ok(())
}

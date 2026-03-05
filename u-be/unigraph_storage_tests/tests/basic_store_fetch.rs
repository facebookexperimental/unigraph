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
fn store_and_fetch_full_graph() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    let graph = TestGraphTimeline::get_nth(42);
    let key = make_graph_time_key("test", "g_42", 1000);

    storage.store_graph_full(&key, &graph)?;

    let fetched = storage.fetch_graph(&key.graph_key())?;
    assert_graphs_equal(&graph, &fetched);

    Ok(())
}

#[test]
fn store_multiple_graphs_and_list_frames() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    for i in 0..5 {
        let graph = TestGraphTimeline::get_nth(i);
        let key = make_graph_time_key("test", &format!("g_{}", i), 1000 + i as i64);
        storage.store_graph_full(&key, &graph)?;
    }

    let frames = storage.graph.list_frames(&TimelineID("test".to_string()))?;
    assert_eq!(frames.len(), 5);

    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base      
----------------------------------------------------------------------
g_0                  1970-01-01T00:16:40Z     Full       -         
g_1                  1970-01-01T00:16:41Z     Full       -         
g_2                  1970-01-01T00:16:42Z     Full       -         
g_3                  1970-01-01T00:16:43Z     Full       -         
g_4                  1970-01-01T00:16:44Z     Full       -         
"
    );

    // Verify each graph can be fetched and matches
    for i in 0..5 {
        let expected = TestGraphTimeline::get_nth(i);
        let fetched = storage.fetch_graph(&make_graph_key("test", &format!("g_{}", i)))?;
        assert_graphs_equal(&expected, &fetched);
    }

    Ok(())
}

#[test]
fn store_empty_frame() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    let key = make_graph_time_key("test", "empty_1", 1000);
    storage.graph.store_frame_empty(&key)?;

    // Verify it's listed
    let frames = storage.graph.list_frames(&TimelineID("test".to_string()))?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].frame_type, FrameType::Empty);

    // Verify fetch returns an error
    let result = storage.fetch_graph(&key.graph_key());
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap());
    assert!(
        err_msg.contains("empty") || err_msg.contains("Empty"),
        "Error should mention empty frame, got: {}",
        err_msg,
    );

    Ok(())
}

#[test]
fn store_error_frame() -> Result<()> {
    use chrono::TimeZone;

    let storage = make_storage();
    setup_timeline(&storage, "test");

    let errors = vec![
        TimestampedError {
            timestamp: chrono::Utc.timestamp_opt(1000, 0).unwrap(),
            message: "First error: graph computation failed".to_string(),
        },
        TimestampedError {
            timestamp: chrono::Utc.timestamp_opt(1001, 0).unwrap(),
            message: "Second error: timeout exceeded".to_string(),
        },
    ];

    let key = make_graph_time_key("test", "err_1", 1000);
    storage.store_error(&key, &errors)?;

    // Verify it's listed as Error
    let frames = storage.graph.list_frames(&TimelineID("test".to_string()))?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].frame_type, FrameType::Error);

    // Verify fetch_graph returns an error
    let result = storage.fetch_graph(&key.graph_key());
    assert!(result.is_err());

    // Verify errors can be fetched back
    let fetched_errors = storage.fetch_errors(&key.graph_key())?;
    assert_eq!(fetched_errors.len(), 2);
    assert_eq!(fetched_errors[0].message, errors[0].message);
    assert_eq!(fetched_errors[1].message, errors[1].message);

    Ok(())
}

#[test]
fn get_frame_metadata_only() -> Result<()> {
    let storage = make_storage();
    setup_timeline(&storage, "test");

    let graph = TestGraphTimeline::get_nth(0);
    let key = make_graph_time_key("test", "g_0", 1000);
    storage.store_graph_full(&key, &graph)?;

    // Fetch without data
    let row = storage
        .graph
        .get_frame(&key.graph_key(), false)?
        .expect("Frame should exist");

    assert_eq!(row.frame_type, FrameType::Full);
    assert!(
        row.data.is_none(),
        "Data should be None for metadata-only fetch"
    );

    // Fetch with data
    let row = storage
        .graph
        .get_frame(&key.graph_key(), true)?
        .expect("Frame should exist");

    assert_eq!(row.frame_type, FrameType::Full);
    assert!(row.data.is_some(), "Data should be Some for full fetch");

    Ok(())
}

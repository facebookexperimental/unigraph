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
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn store_and_fetch_full_graph() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let graph = TestGraphTimeline::get_nth(42);
    let key = make_graph_time_key("test", 42, 1000);

    db.store_graph_full(&key, &graph).await?;

    let fetched = db.fetch_graph(&key.graph_key()).await?;
    assert_graphs_equal(&graph, &fetched);

    Ok(())
}

#[tokio::test]
async fn store_multiple_graphs_and_list_frames() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    for i in 0..5 {
        let graph = TestGraphTimeline::get_nth(i);
        let key = make_graph_time_key("test", i as i64, 1000 + i as i64);
        db.store_graph_full(&key, &graph).await?;
    }

    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    assert_eq!(frames.len(), 5);

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

    // Verify each graph can be fetched and matches
    for i in 0..5 {
        let expected = TestGraphTimeline::get_nth(i);
        let fetched = db.fetch_graph(&make_graph_key("test", i as i64)).await?;
        assert_graphs_equal(&expected, &fetched);
    }

    Ok(())
}

#[tokio::test]
async fn store_empty_frame() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let key = make_graph_time_key("test", 1, 1000);
    db.store_frame_empty(&key).await?;

    // Verify it's listed
    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].frame_type, FrameType::Empty);

    // Verify fetch returns an error
    let result = db.fetch_graph(&key.graph_key()).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap());
    assert!(
        err_msg.contains("empty") || err_msg.contains("Empty") || err_msg.contains("missing"),
        "Error should mention empty/missing frame, got: {}",
        err_msg,
    );

    Ok(())
}

#[tokio::test]
async fn store_error_frame() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let errors = vec![
        TimestampedError {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000),
            message: "First error: graph computation failed".to_string(),
        },
        TimestampedError {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1001),
            message: "Second error: timeout exceeded".to_string(),
        },
    ];

    let key = make_graph_time_key("test", 1, 1000);
    db.store_error(&key, &errors).await?;

    // Verify it's listed as Error
    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].frame_type, FrameType::Error);

    // Verify fetch_graph returns an error
    let result = db.fetch_graph(&key.graph_key()).await;
    assert!(result.is_err());

    // Verify errors can be fetched back
    let fetched_errors = db.fetch_errors(&key.graph_key()).await?;
    assert_eq!(fetched_errors.len(), 2);
    assert_eq!(fetched_errors[0].message, errors[0].message);
    assert_eq!(fetched_errors[1].message, errors[1].message);

    Ok(())
}

#[tokio::test]
async fn delete_frame() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.store_graph_full(&key, &graph).await?;

    // Before deletion: frame exists, no blobs pending cleanup
    let frames = db.list_frames(&timeline_id).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
1                    1970-01-01T00:16:40.000Z Full -
"
    );

    let pending = db.get_blobs_pending_cleanup().await?;
    snapshot!(format_blob_keys(&pending), "");

    // Delete it
    let deleted = db.delete_frame(&key.graph_key(), &timeline_id).await?;
    assert!(deleted);

    // After deletion: no frames, no pending blobs (inline blobs disappear with the row)
    let frames = db.list_frames(&timeline_id).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
"
    );

    let pending = db.get_blobs_pending_cleanup().await?;
    snapshot!(format_blob_keys(&pending), "");

    // Second delete returns false (idempotent)
    let deleted = db.delete_frame(&key.graph_key(), &timeline_id).await?;
    assert!(!deleted);

    Ok(())
}

#[tokio::test]
async fn delete_frame_with_external_blobs() -> Result<()> {
    use unigraph_storage_core::UnigraphBlobStorage;

    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());

    // Create timeline with External blob storage — forces all blobs to external storage
    db.create_timeline(
        &TimelineID("test".to_string()),
        &TimelineConfig {
            schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            external_id_namespace: None,
            blob_storage: BlobStorageMode::External,
        },
    )
    .await
    .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.store_graph_full(&key, &graph).await?;

    // Before deletion: frame exists
    let frames = db.list_frames(&timeline_id).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
1                    1970-01-01T00:16:40.000Z Full -
"
    );

    // External blobs should exist in blob storage
    let blobs = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs),
        "
test/1/_manifest.json
test/1/directed_6393609996011794433
test/1/directed_offsets_194495599743061723
test/1/dynamic_5399181262045623723
test/1/entry_points_252579103958576740
test/1/metrics_16688319674661559295
test/1/node_names_14937681792348936820
test/1/node_names_offsets_5236345249475374570
test/1/tag_sets_15326796702629799067
test/1/tagged_6861884023275401222
test/1/traversal_config_252579103958576740
"
    );

    // No pending cleanup (blobs were unregistered after successful store)
    let pending = db.get_blobs_pending_cleanup().await?;
    snapshot!(format_blob_keys(&pending), "");

    // Graph should be fetchable from external blobs
    let fetched = db.fetch_graph(&key.graph_key()).await?;
    assert_graphs_equal(&graph, &fetched);

    // Delete the frame
    let deleted = db.delete_frame(&key.graph_key(), &timeline_id).await?;
    assert!(deleted);

    // After deletion: no frames
    let frames = db.list_frames(&timeline_id).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base
----------------------------------------------------------------------
"
    );

    // External blobs still exist (sweeper hasn't run yet)
    let blobs_after = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs_after),
        "
test/1/_manifest.json
test/1/directed_6393609996011794433
test/1/directed_offsets_194495599743061723
test/1/dynamic_5399181262045623723
test/1/entry_points_252579103958576740
test/1/metrics_16688319674661559295
test/1/node_names_14937681792348936820
test/1/node_names_offsets_5236345249475374570
test/1/tag_sets_15326796702629799067
test/1/tagged_6861884023275401222
test/1/traversal_config_252579103958576740
"
    );

    // But blob keys are registered for cleanup
    let pending = db.get_blobs_pending_cleanup().await?;
    snapshot!(
        format_blob_keys(&pending),
        "
test/1/_manifest.json
test/1/directed_6393609996011794433
test/1/directed_offsets_194495599743061723
test/1/dynamic_5399181262045623723
test/1/entry_points_252579103958576740
test/1/metrics_16688319674661559295
test/1/node_names_14937681792348936820
test/1/node_names_offsets_5236345249475374570
test/1/tag_sets_15326796702629799067
test/1/tagged_6861884023275401222
test/1/traversal_config_252579103958576740
"
    );

    Ok(())
}

#[tokio::test]
async fn get_frame_metadata_only() -> Result<()> {
    let db = make_db();
    setup_timeline(&db, "test").await;

    let graph = TestGraphTimeline::get_nth(0);
    let key = make_graph_time_key("test", 0, 1000);
    db.store_graph_full(&key, &graph).await?;

    // Fetch without data
    let row = db
        .get_frame(&key.graph_key(), false)
        .await?
        .expect("Frame should exist");

    assert_eq!(row.frame_type, FrameType::Full);
    assert!(
        row.data.is_none(),
        "Data should be None for metadata-only fetch"
    );

    // Fetch with data
    let row = db
        .get_frame(&key.graph_key(), true)
        .await?
        .expect("Frame should exist");

    assert_eq!(row.frame_type, FrameType::Full);
    assert!(row.data.is_some(), "Data should be Some for full fetch");

    Ok(())
}

#[tokio::test]
async fn sweep_deleted_blobs() -> Result<()> {
    use unigraph_storage_core::UnigraphBlobStorage;

    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());

    // Create timeline with External blob storage
    db.create_timeline(
        &TimelineID("test".to_string()),
        &TimelineConfig {
            schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            external_id_namespace: None,
            blob_storage: BlobStorageMode::External,
        },
    )
    .await
    .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.store_graph_full(&key, &graph).await?;

    // After store: blobs exist in external storage, nothing pending cleanup
    let blobs = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs),
        "
test/1/_manifest.json
test/1/directed_6393609996011794433
test/1/directed_offsets_194495599743061723
test/1/dynamic_5399181262045623723
test/1/entry_points_252579103958576740
test/1/metrics_16688319674661559295
test/1/node_names_14937681792348936820
test/1/node_names_offsets_5236345249475374570
test/1/tag_sets_15326796702629799067
test/1/tagged_6861884023275401222
test/1/traversal_config_252579103958576740
"
    );
    let pending = db.get_blobs_pending_cleanup().await?;
    snapshot!(format_blob_keys(&pending), "");

    // Delete the frame — blobs registered for cleanup but still physically present
    db.delete_frame(&key.graph_key(), &timeline_id).await?;

    let blobs_after_delete = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs_after_delete),
        "
test/1/_manifest.json
test/1/directed_6393609996011794433
test/1/directed_offsets_194495599743061723
test/1/dynamic_5399181262045623723
test/1/entry_points_252579103958576740
test/1/metrics_16688319674661559295
test/1/node_names_14937681792348936820
test/1/node_names_offsets_5236345249475374570
test/1/tag_sets_15326796702629799067
test/1/tagged_6861884023275401222
test/1/traversal_config_252579103958576740
"
    );
    let pending = db.get_blobs_pending_cleanup().await?;
    snapshot!(
        format_blob_keys(&pending),
        "
test/1/_manifest.json
test/1/directed_6393609996011794433
test/1/directed_offsets_194495599743061723
test/1/dynamic_5399181262045623723
test/1/entry_points_252579103958576740
test/1/metrics_16688319674661559295
test/1/node_names_14937681792348936820
test/1/node_names_offsets_5236345249475374570
test/1/tag_sets_15326796702629799067
test/1/tagged_6861884023275401222
test/1/traversal_config_252579103958576740
"
    );

    // Sweep with Duration::ZERO — should sweep everything
    let swept = db.sweep_blobs(std::time::Duration::ZERO).await?;
    assert_eq!(swept, 11);

    // After sweep: blobs physically gone, cleanup table empty
    let blobs_after_sweep = sqlite.list_blobs("").await?;
    snapshot!(format_blob_keys(&blobs_after_sweep), "");

    let pending_after_sweep = db.get_blobs_pending_cleanup().await?;
    snapshot!(format_blob_keys(&pending_after_sweep), "");

    // Sweeping again is a no-op
    let swept_again = db.sweep_blobs(std::time::Duration::ZERO).await?;
    assert_eq!(swept_again, 0);

    Ok(())
}

#[tokio::test]
async fn sweep_respects_min_age() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());

    // Create timeline with External blob storage
    db.create_timeline(
        &TimelineID("test".to_string()),
        &TimelineConfig {
            schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            external_id_namespace: None,
            blob_storage: BlobStorageMode::External,
        },
    )
    .await
    .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.store_graph_full(&key, &graph).await?;
    db.delete_frame(&key.graph_key(), &timeline_id).await?;

    // Sweep with a large min_age (1 hour) — nothing should be old enough
    let swept = db.sweep_blobs(std::time::Duration::from_secs(3600)).await?;
    assert_eq!(swept, 0);

    // Pending cleanup still has entries
    let pending = db.get_blobs_pending_cleanup().await?;
    assert!(!pending.is_empty());

    Ok(())
}

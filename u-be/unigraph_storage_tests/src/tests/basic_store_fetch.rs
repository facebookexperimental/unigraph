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
async fn store_and_fetch_full_graph() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    let graph = TestGraphTimeline::get_nth(42);
    let key = make_graph_time_key("test", 42, 1000);

    db.graph.store(&key, &graph, None, &task).await?;

    let fetched = db.graph.fetch(&key.graph_key(), &task).await?;
    assert_graphs_equal(&graph, &fetched);

    Ok(())
}

#[tokio::test]
async fn store_multiple_graphs_and_list_frames() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    for i in 0..5 {
        let graph = TestGraphTimeline::get_nth(i);
        let key = make_graph_time_key("test", i as i64, 1000 + i as i64);
        db.graph.store(&key, &graph, None, &task).await?;
    }

    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    assert_eq!(frames.len(), 5);

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

    // Verify each graph can be fetched and matches
    for i in 0..5 {
        let expected = TestGraphTimeline::get_nth(i);
        let fetched = db
            .graph
            .fetch(&make_graph_key("test", i as i64), &task)
            .await?;
        assert_graphs_equal(&expected, &fetched);
    }

    Ok(())
}

#[tokio::test]
async fn store_empty_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    let tl = TimelineID("test".to_string());
    let empty_frames = vec![Frame {
        timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000),
        graph_id: GraphID(1),
    }];
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, empty_frames, false, &task)
        .await?;

    let key = make_graph_time_key("test", 1, 1000);

    // Verify it's listed
    let frames = db.frames.list(&tl, &task).await?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].frame_type, FrameType::Empty);

    // Verify fetch returns an error
    let result = db.graph.fetch(&key.graph_key(), &task).await;
    assert!(result.is_err());
    let err_msg = format!("{:#}", result.err().unwrap());
    assert!(
        err_msg.contains("empty")
            || err_msg.contains("Empty")
            || err_msg.contains("missing")
            || err_msg.contains("no Full frame found"),
        "Error should mention empty/missing frame or no Full frame, got: {}",
        err_msg,
    );

    Ok(())
}

#[tokio::test]
async fn store_error_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

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
    db.graph.store_error(&key, &errors, &task).await?;

    // Verify it's listed as Error
    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].frame_type, FrameType::Error);

    // Verify fetch_graph returns an error
    let result = db.graph.fetch(&key.graph_key(), &task).await;
    assert!(result.is_err());

    // Verify errors can be fetched back
    let fetched_errors = db.graph.fetch_errors(&key.graph_key(), &task).await?;
    assert_eq!(fetched_errors.len(), 2);
    assert_eq!(fetched_errors[0].message, errors[0].message);
    assert_eq!(fetched_errors[1].message, errors[1].message);

    Ok(())
}

#[tokio::test]
async fn delete_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.graph.store(&key, &graph, None, &task).await?;

    // Before deletion: frame exists, no blobs pending cleanup
    let frames = db.frames.list(&timeline_id, &task).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
1                    1970-01-01T00:16:40.000Z Full -          -
"
    );

    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(format_blob_keys(&pending), "");

    // Delete it
    let deleted = db
        .graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;
    assert!(deleted);

    // After deletion: no frames, no pending blobs (inline blobs disappear with the row)
    let frames = db.frames.list(&timeline_id, &task).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
"
    );

    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(format_blob_keys(&pending), "");

    // Second delete returns false (idempotent)
    let deleted = db
        .graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;
    assert!(!deleted);

    Ok(())
}

#[tokio::test]
async fn delete_frame_with_external_blobs() -> Result<()> {
    use unigraph_storage_core::UnigraphBlobStorage;

    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    let task = ll::Task::create_new("test");

    // Create timeline with External blob storage — forces all blobs to external storage
    db.timelines
        .create(
            &TimelineID("test".to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            &task,
        )
        .await
        .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.graph.store(&key, &graph, None, &task).await?;

    // Before deletion: frame exists
    let frames = db.frames.list(&timeline_id, &task).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
1                    1970-01-01T00:16:40.000Z Full -          -
"
    );

    // External blobs should exist in blob storage
    let blobs = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    // No pending cleanup (blobs were unregistered after successful store)
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(format_blob_keys(&pending), "");

    // Graph should be fetchable from external blobs
    let fetched = db.graph.fetch(&key.graph_key(), &task).await?;
    assert_graphs_equal(&graph, &fetched);

    // Delete the frame
    let deleted = db
        .graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;
    assert!(deleted);

    // After deletion: no frames
    let frames = db.frames.list(&timeline_id, &task).await?;
    snapshot!(
        format_frames_table(&frames),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
"
    );

    // External blobs still exist (sweeper hasn't run yet)
    let blobs_after = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs_after),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    // But blob keys are registered for cleanup
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(
        format_blob_keys(&pending),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    Ok(())
}

#[tokio::test]
async fn get_frame_metadata_only() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    let graph = TestGraphTimeline::get_nth(0);
    let key = make_graph_time_key("test", 0, 1000);
    db.graph.store(&key, &graph, None, &task).await?;

    // Fetch without data
    let row = db
        .frames
        .get(&key.graph_key(), false, &task)
        .await?
        .expect("Frame should exist");

    assert_eq!(row.frame_type, FrameType::Full);
    assert!(
        row.manifest_json.is_none() && row.inline_blobs.is_none(),
        "Payload should be unpopulated for a metadata-only fetch"
    );
    assert!(
        row.blobs_are_inline.is_none(),
        "A metadata-only fetch never looks at the payload columns, so it cannot \
         say whether the blobs are inline"
    );

    // Fetch with data
    let row = db
        .frames
        .get(&key.graph_key(), true, &task)
        .await?
        .expect("Frame should exist");

    assert_eq!(row.frame_type, FrameType::Full);
    assert!(
        row.manifest_json.is_some(),
        "Manifest should be Some for full fetch"
    );
    assert_eq!(
        row.blobs_are_inline,
        Some(true),
        "The test timeline stores blobs inline, and a full fetch knows it"
    );

    Ok(())
}

#[tokio::test]
async fn sweep_deleted_blobs() -> Result<()> {
    use unigraph_storage_core::UnigraphBlobStorage;

    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    let task = ll::Task::create_new("test");

    // Create timeline with External blob storage
    db.timelines
        .create(
            &TimelineID("test".to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            &task,
        )
        .await
        .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.graph.store(&key, &graph, None, &task).await?;

    // After store: blobs exist in external storage, nothing pending cleanup
    let blobs = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(format_blob_keys(&pending), "");

    // Delete the frame — blobs registered for cleanup but still physically present
    db.graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;

    let blobs_after_delete = sqlite.list_blobs("").await?;
    snapshot!(
        format_blob_keys(&blobs_after_delete),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(
        format_blob_keys(&pending),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    // Sweep with Duration::ZERO — should sweep everything
    let swept = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await?;
    assert_eq!(swept, 12);

    // After sweep: blobs physically gone, cleanup table empty
    let blobs_after_sweep = sqlite.list_blobs("").await?;
    snapshot!(format_blob_keys(&blobs_after_sweep), "");

    let pending_after_sweep = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(format_blob_keys(&pending_after_sweep), "");

    // Sweeping again is a no-op
    let swept_again = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await?;
    assert_eq!(swept_again, 0);

    Ok(())
}

#[tokio::test]
async fn sweep_respects_min_age() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    let task = ll::Task::create_new("test");

    // Create timeline with External blob storage
    db.timelines
        .create(
            &TimelineID("test".to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            &task,
        )
        .await
        .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    db.graph.store(&key, &graph, None, &task).await?;
    db.graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;

    // Sweep with a large min_age (1 hour) — nothing should be old enough
    let swept = db
        .blob_storage
        .sweep(std::time::Duration::from_secs(3600), None, &task)
        .await?;
    assert_eq!(swept, 0);

    // Pending cleanup still has entries
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    assert!(!pending.is_empty());

    Ok(())
}

#[tokio::test]
async fn sweep_respects_limit() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    let task = ll::Task::create_new("test");

    // External blob storage so every blob lands in the cleanup queue on delete.
    db.timelines
        .create(
            &TimelineID("test".to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            &task,
        )
        .await
        .unwrap();

    let graph = TestGraphTimeline::get_nth(1);
    let key = make_graph_time_key("test", 1, 1000);
    let timeline_id = TimelineID("test".to_string());

    // Store + delete one frame: registers its 12 external blobs for cleanup.
    // (The delete's own piggybacked sweep uses a 2h min_age, so these
    // freshly-registered blobs are left untouched — all 12 stay pending.)
    db.graph.store(&key, &graph, None, &task).await?;
    db.graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(
        format_blob_keys(&pending),
        "
graphs/test/1/_manifest.json
graphs/test/1/csr_edges_6081454111982381661
graphs/test/1/csr_offsets_5554658389978294871
graphs/test/1/edge_metadata_7494857868300892803
graphs/test/1/edge_metadata_map_15850574455260760061
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    // A limited sweep processes at most `limit` blobs; the rest stay pending.
    let swept = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, Some(5), &task)
        .await?;
    assert_eq!(swept, 5);
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(
        format_blob_keys(&pending),
        "
graphs/test/1/entry_points_9535545603450022154
graphs/test/1/labels_8004798928044272053
graphs/test/1/metrics_10211407828568219209
graphs/test/1/node_names_2944712204298532354
graphs/test/1/node_names_offsets_14475185708095569284
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    // Repeated limited sweeps drain the backlog incrementally.
    let swept = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, Some(5), &task)
        .await?;
    assert_eq!(swept, 5);
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(
        format_blob_keys(&pending),
        "
graphs/test/1/properties_4370653166743570923
graphs/test/1/traversal_config_9535545603450022154
"
    );

    // A limit larger than the remaining backlog sweeps only what's left.
    let swept = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, Some(5), &task)
        .await?;
    assert_eq!(swept, 2);
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(format_blob_keys(&pending), "");

    Ok(())
}

#[tokio::test]
async fn replace_empty_frames_with_full_graphs() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_timeline(&db, "test", &task).await;

    // Phase A: Register empty frames for 5 commits (mimics ingestion pipeline).
    // All graph_ids are allocated up front, so the last frame has graph_id=4.
    let tl = TimelineID("test".to_string());
    let empty_frames: Vec<Frame> = (0..5)
        .map(|i| Frame {
            timestamp: unigraph_timestamp::Timestamp::from_unix_timestamp(1000 + i),
            graph_id: GraphID(i),
        })
        .collect();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&tl, empty_frames, false, &task)
        .await?;

    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    assert_eq!(frames.len(), 5);
    assert!(frames.iter().all(|f| f.frame_type == FrameType::Empty));

    // Phase B: Replace each empty frame with a Full graph, starting from
    // graph_id=0. This is NOT an append (graph_id=0 < last graph_id=4),
    // but it should succeed because we're replacing existing frames.
    for i in 0..5 {
        let graph = TestGraphTimeline::get_nth(i as u64);
        let key = make_graph_time_key("test", i, 1000 + i);
        db.graph.store(&key, &graph, None, &task).await?;
    }

    // Verify all frames are now Full.
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

    // Verify each graph can be fetched back correctly.
    for i in 0..5 {
        let expected = TestGraphTimeline::get_nth(i as u64);
        let fetched = db.graph.fetch(&make_graph_key("test", i), &task).await?;
        assert_graphs_equal(&expected, &fetched);
    }

    Ok(())
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use k9::snapshot;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;
use unigraph_timestamp::Timestamp;

fn test_config_key() -> TraversalConfigKey {
    TraversalConfigKey::from_blob(b"test-config-data")
}

fn make_db() -> (UnigraphDb, Arc<SqliteStorage>) {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    (db, sqlite)
}

fn make_db_external_blobs() -> (UnigraphDb, Arc<SqliteStorage>) {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    (db, sqlite)
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

async fn setup_timeline_external_blobs(db: &UnigraphDb, name: &str, task: &ll::Task) {
    db.timelines
        .create(
            &TimelineID(name.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            task,
        )
        .await
        .unwrap();
}

/// Sanitize expires_after timestamps in frame rows for deterministic snapshots.
/// Replaces the actual timestamp with a placeholder since it depends on wall clock.
fn sanitize_expires_after(frames: &[FrameRow]) -> Vec<String> {
    frames
        .iter()
        .map(|f| {
            let expires = match &f.expires_after {
                Some(_) => "<set>",
                None => "<none>",
            };
            format!(
                "graph_id={} type={} expires_after={}",
                f.frame.graph_id.0, f.frame_type, expires
            )
        })
        .collect()
}

#[tokio::test]
async fn ttl_expired_frames_and_configs_cleanup() -> Result<()> {
    let (db, sqlite) = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = TimelineID("test".to_string());
    setup_timeline(&db, "test", &task).await;

    let expires_at = Timestamp::now().add_duration(Duration::from_secs(1))?;

    // Store graph A normally, then re-insert with expires_at
    let graph_a = TestGraphTimeline::get_nth(0);
    let key_a = make_graph_time_key("test", 0, 1000);
    db.graph.store(&key_a, &graph_a, None, &task).await?;

    // Select the frame data, delete, re-insert with TTL
    {
        let frame = db
            .frames
            .get(&key_a.graph_key(), true, &task)
            .await?
            .unwrap();
        let data = frame.data.unwrap();
        let mut conn = db.graph_conn_write().await?;
        conn.delete_frame(&key_a.graph_key(), &task).await?;
        conn.store_frame(
            &key_a,
            FrameType::Full,
            None,
            &data.manifest_json,
            data.inline_blobs.as_deref(),
            Some(expires_at),
            &task,
        )
        .await?;
    }

    // Store graph B without TTL (normal path)
    let graph_b = TestGraphTimeline::get_nth(1);
    let key_b = make_graph_time_key("test", 1, 1001);
    db.graph.store(&key_b, &graph_b, None, &task).await?;

    // Store a config with 1-second TTL
    {
        let mut conn = db.graph_conn_write().await?;
        let config_key = test_config_key();
        conn.store_traversal_config(
            &config_key,
            Some(b"test-blob-data"),
            None,
            Some(expires_at),
            &task,
        )
        .await?;
    }

    // Before expiration: both frames exist, nothing expired
    let frames_before = db.frames.list(&timeline_id, &task).await?;
    assert_eq!(frames_before.len(), 2, "both frames should exist");

    let expired_frames_before = db.utility.get_expired_frames(&timeline_id, &task).await?;
    snapshot!(sanitize_expires_after(&expired_frames_before), "[]");

    let expired_configs_before = db.utility.get_expired_configs(&task).await?;
    snapshot!(expired_configs_before, "[]");

    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_secs(2)).await;

    // After expiration: expired queries should return results
    let expired_frames = db.utility.get_expired_frames(&timeline_id, &task).await?;
    snapshot!(
        sanitize_expires_after(&expired_frames),
        r#"
[
    "graph_id=0 type=Full expires_after=<set>",
]
"#
    );

    let expired_configs = db.utility.get_expired_configs(&task).await?;
    snapshot!(
        expired_configs,
        r#"
[
    "tvc-13d1f86dda51b96f",
]
"#
    );

    // Both frames still exist (cleanup hasn't run)
    let frames_still = db.frames.list(&timeline_id, &task).await?;
    assert_eq!(frames_still.len(), 2, "cleanup hasn't run yet");

    // Run cleanup (use Duration::ZERO for sweep_min_age in tests)
    let result = db
        .utility
        .cleanup_expired(Some(Duration::ZERO), &task)
        .await?;
    assert_eq!(result.frames_deleted, 1);
    assert_eq!(result.configs_deleted, 1);

    // After cleanup: only graph B remains
    let frames_after = db.frames.list(&timeline_id, &task).await?;
    snapshot!(
        sanitize_expires_after(&frames_after),
        r#"
[
    "graph_id=1 type=Full expires_after=<none>",
]
"#
    );

    // Config is gone
    let mut conn = db.graph_conn().await?;
    let config = conn.get_traversal_config(&test_config_key(), &task).await?;
    assert!(config.is_none(), "config should be deleted");
    drop(conn);

    // No orphaned blobs (inline storage, so nothing in blob storage)
    let blobs = sqlite.list_blobs("").await?;
    snapshot!(blobs, "[]");

    // No pending cleanup
    let pending = db.blob_storage.get_pending_cleanup(&task).await?;
    snapshot!(pending, "[]");

    // Nothing left to expire
    let expired_after = db.utility.get_expired_frames(&timeline_id, &task).await?;
    snapshot!(sanitize_expires_after(&expired_after), "[]");

    Ok(())
}

#[tokio::test]
async fn ttl_cleanup_with_external_blobs() -> Result<()> {
    let (db, sqlite) = make_db_external_blobs();
    let task = ll::Task::create_new("test");
    let timeline_id = TimelineID("test".to_string());
    setup_timeline_external_blobs(&db, "test", &task).await;

    let expires_at = Timestamp::now().add_duration(Duration::from_secs(1))?;

    // Store graph with external blobs and TTL
    let graph = TestGraphTimeline::get_nth(0);
    let key = make_graph_time_key("test", 0, 1000);
    db.graph.store(&key, &graph, None, &task).await?;

    // Verify blobs were stored externally
    let blobs_before = sqlite.list_blobs("").await?;
    assert!(
        !blobs_before.is_empty(),
        "blobs should be in external storage"
    );

    // Re-insert with TTL (delete + store with expires_at)
    {
        let frame = db.frames.get(&key.graph_key(), true, &task).await?.unwrap();
        let data = frame.data.unwrap();
        let mut conn = db.graph_conn_write().await?;
        conn.delete_frame(&key.graph_key(), &task).await?;
        // Unregister the blobs that were registered during delete — we're
        // re-inserting the same frame, so the blobs are still valid.
        conn.unregister_blobs_for_cleanup(
            &blobs_before
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            &task,
        )
        .await?;
        conn.store_frame(
            &key,
            FrameType::Full,
            None,
            &data.manifest_json,
            None, // blobs are external
            Some(expires_at),
            &task,
        )
        .await?;
    }

    // Store a non-TTL graph too
    let graph_b = TestGraphTimeline::get_nth(1);
    let key_b = make_graph_time_key("test", 1, 1001);
    db.graph.store(&key_b, &graph_b, None, &task).await?;

    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Run cleanup
    let result = db
        .utility
        .cleanup_expired(Some(Duration::ZERO), &task)
        .await?;
    assert_eq!(result.frames_deleted, 1);

    // Only graph B remains
    let frames_after = db.frames.list(&timeline_id, &task).await?;
    assert_eq!(frames_after.len(), 1);
    assert_eq!(frames_after[0].frame.graph_id, GraphID(1));

    // Blobs from the expired graph should be swept (pending cleanup → deleted)
    // The non-expired graph's blobs should still be present
    let remaining_blobs = sqlite.list_blobs("").await?;
    // Only graph B's blobs should remain
    assert!(
        remaining_blobs.iter().all(|b| b.contains("/1/")),
        "only graph B's blobs should remain, got: {:?}",
        remaining_blobs
    );

    Ok(())
}

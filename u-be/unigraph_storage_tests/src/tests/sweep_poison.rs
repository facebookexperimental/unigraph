// Copyright (c) Meta Platforms, Inc. and affiliates.

//! One undeletable blob must cost one blob, not the whole cleanup queue.
//!
//! Deleting and unregistering are separate steps and only the first is per-key,
//! so an all-or-nothing unregister let a single bad key abort the batch *after*
//! its siblings' deletes had already landed. The blobs were gone, the queue
//! still listed them, and the next sweep repeated the whole thing — grinding
//! over the same window forever and re-deleting anything that reused a key.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;

use crate::*;

/// Blob storage that refuses to delete one chosen key, the way a permissions
/// failure or a backend bug would. Everything else delegates.
struct PoisonedBlobStorage {
    inner: Arc<SqliteStorage>,
    undeletable: String,
}

#[async_trait]
impl UnigraphBlobStorage for PoisonedBlobStorage {
    async fn put_blob(&self, key: &str, data: &[u8]) -> Result<()> {
        self.inner.put_blob(key, data).await
    }

    async fn get_blob(&self, key: &str) -> Result<Vec<u8>> {
        self.inner.get_blob(key).await
    }

    async fn delete_blob(&self, key: &str) -> Result<()> {
        anyhow::ensure!(key != self.undeletable, "[403] Permission denied");
        self.inner.delete_blob(key).await
    }

    async fn has_blob(&self, key: &str) -> Result<bool> {
        self.inner.has_blob(key).await
    }

    async fn list_blobs(&self, prefix: &str) -> Result<Vec<String>> {
        self.inner.list_blobs(prefix).await
    }
}

#[tokio::test]
async fn one_undeletable_blob_does_not_wedge_the_queue() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let task = ll::Task::create_new("test");
    let timeline_id = TimelineID("test".to_string());

    // Store a frame with external blobs through a plain db, then pick one of
    // its blobs to be undeletable.
    let plain = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    setup_external_timeline(&plain, &timeline_id, &task).await?;
    let key = make_graph_time_key("test", 1, 1000);
    plain
        .graph
        .store(&key, &TestGraphTimeline::get_nth(1), None, &task)
        .await?;
    let stored = sqlite.list_blobs("").await?;
    let undeletable = stored
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("External mode should have uploaded blobs"))?;

    let db = UnigraphDb::new(
        sqlite.clone(),
        Arc::new(PoisonedBlobStorage {
            inner: sqlite.clone(),
            undeletable: undeletable.clone(),
        }),
    );

    // Deleting the frame queues every one of its blobs.
    db.graph
        .delete(&key.graph_key(), &timeline_id, &task)
        .await?;
    let queued = db.blob_storage.get_pending_cleanup(&task).await?;
    assert_eq!(
        queued.len(),
        stored.len(),
        "the frame's blobs should all be queued"
    );

    // The sweep reports the failure — loosely asserted on purpose, so the
    // checks that follow are the ones carrying this test.
    let swept = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await;
    let error = format!("{:#}", swept.unwrap_err());
    assert!(
        error.contains("Permission denied"),
        "the sweep should surface what went wrong, got: {error}"
    );

    // ...but it still made progress. Only the bad key is left, on both sides.
    assert_eq!(
        db.blob_storage.get_pending_cleanup(&task).await?,
        vec![undeletable.clone()],
        "everything the sweep did delete must be crossed off the queue"
    );
    assert_eq!(
        sqlite.list_blobs("").await?,
        vec![undeletable.clone()],
        "everything except the bad key should be physically gone"
    );

    // A second sweep has one key left to try, not the original batch. Without
    // per-key unregister this is where the grinding started.
    let second = db
        .blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await;
    assert!(second.is_err(), "the bad key still cannot be deleted");
    assert_eq!(
        db.blob_storage.get_pending_cleanup(&task).await?,
        vec![undeletable],
        "and it stays queued rather than being silently dropped"
    );

    Ok(())
}

async fn setup_external_timeline(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<()> {
    db.timelines
        .create(
            timeline_id,
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: BlobStorageMode::External,
                store_metric_history: None,
            },
            task,
        )
        .await
}

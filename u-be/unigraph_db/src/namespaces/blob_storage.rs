// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Blob storage lifecycle — sweep orphaned blobs and inspect cleanup queue.

use std::sync::Arc;

use anyhow::Result;
use ll::task;

use crate::storage::UnigraphStorage;

/// Handle for blob storage lifecycle operations.
///
/// Obtained via [`UnigraphDb::blob_storage`](crate::UnigraphDb).
#[derive(Clone)]
pub struct BlobStorageOps {
    pub(crate) storage: Arc<UnigraphStorage>,
}

impl BlobStorageOps {
    /// Sweep external blobs that have been pending cleanup for at least `min_age`.
    ///
    /// Call this periodically (e.g., every few minutes) to clean up orphaned
    /// blobs from deleted frames. Use `Duration::ZERO` in tests to sweep
    /// immediately.
    ///
    /// Returns the number of blobs swept.
    #[task(tags(l3))]
    pub async fn sweep(&self, min_age: std::time::Duration, task: &ll::Task) -> Result<usize> {
        self.storage.sweep_blobs(min_age, &task).await
    }

    /// Get all blob keys that are pending cleanup.
    #[task(tags(l3))]
    pub async fn get_pending_cleanup(&self, task: &ll::Task) -> Result<Vec<String>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.get_blobs_pending_cleanup(&task).await
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Blob storage lifecycle — sweep orphaned blobs and inspect cleanup queue.

use anyhow::Result;
use ll::task;

use crate::context::UnigraphDbContext;

/// Handle for blob storage lifecycle operations.
///
/// Obtained via [`UnigraphDb::blob_storage`](crate::UnigraphDb).
#[derive(Clone)]
pub struct BlobStorageOps {
    pub(crate) ctx: UnigraphDbContext,
}

impl BlobStorageOps {
    /// Sweep external blobs that have been pending cleanup for at least `min_age`.
    ///
    /// Call this periodically (e.g., every few minutes) to clean up orphaned
    /// blobs from deleted frames. Use `Duration::ZERO` in tests to sweep
    /// immediately.
    ///
    /// `limit` caps how many blobs a single sweep processes (`None` = no cap).
    ///
    /// Returns the number of blobs swept.
    #[task(tags(l3))]
    pub async fn sweep(
        &self,
        min_age: std::time::Duration,
        limit: Option<i64>,
        task: &ll::Task,
    ) -> Result<usize> {
        self.ctx.storage.sweep_blobs(min_age, limit, &task).await
    }

    /// Get all blob keys that are pending cleanup.
    #[task(tags(l3))]
    pub async fn get_pending_cleanup(&self, task: &ll::Task) -> Result<Vec<String>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.get_blobs_pending_cleanup(&task).await
    }
}

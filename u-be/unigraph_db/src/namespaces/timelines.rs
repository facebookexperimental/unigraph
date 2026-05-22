// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Timeline management — create, configure, and list timelines.

use anyhow::Result;
use ll::task;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;

use crate::context::UnigraphDbContext;

/// Handle for timeline operations.
///
/// Obtained via [`UnigraphDb::timelines`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Timelines {
    pub(crate) ctx: UnigraphDbContext,
}

impl Timelines {
    /// Create a new timeline with the given configuration.
    #[task(tags(l3))]
    pub async fn create(
        &self,
        timeline_id: &TimelineID,
        config: &TimelineConfig,
        task: &ll::Task,
    ) -> Result<()> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.create_timeline(timeline_id, config, &task).await
    }

    /// Get the configuration for an existing timeline.
    /// Returns `None` if the timeline does not exist.
    #[task(tags(l3))]
    pub async fn get_config(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<Option<TimelineConfig>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.get_timeline_config(timeline_id, &task).await
    }

    /// List all timeline IDs.
    #[task(tags(l3))]
    pub async fn list(&self, task: &ll::Task) -> Result<Vec<TimelineID>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.list_timelines(&task).await
    }
}

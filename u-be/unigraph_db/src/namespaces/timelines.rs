// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Timeline management — create, configure, and list timelines.

use std::sync::Arc;

use anyhow::Result;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;

use crate::storage::UnigraphStorage;

/// Handle for timeline operations.
///
/// Obtained via [`UnigraphDb::timelines`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Timelines {
    pub(crate) storage: Arc<UnigraphStorage>,
}

impl Timelines {
    /// Create a new timeline with the given configuration.
    pub async fn create(&self, timeline_id: &TimelineID, config: &TimelineConfig) -> Result<()> {
        let mut conn = self.storage.graph.conn().await?;
        conn.create_timeline(timeline_id, config).await
    }

    /// Get the configuration for an existing timeline.
    /// Returns `None` if the timeline does not exist.
    pub async fn get_config(&self, timeline_id: &TimelineID) -> Result<Option<TimelineConfig>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.get_timeline_config(timeline_id).await
    }

    /// List all timeline IDs.
    pub async fn list(&self) -> Result<Vec<TimelineID>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.list_timelines().await
    }
}

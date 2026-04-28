// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Fetch a timeline's configuration and print it as pretty-printed JSON.
#[derive(Parser, Debug)]
pub struct TimelinesGet {
    /// Timeline ID to fetch
    timeline_id: String,
}

impl TimelinesGet {
    pub fn new(timeline_id: String) -> Self {
        Self { timeline_id }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let timeline_id = TimelineID(self.timeline_id.clone());
        let config = ctx
            .db
            .timelines
            .get_config(&timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline '{}' not found", self.timeline_id))?;

        ctx.println_after_done(&serde_json::to_string_pretty(&config)?)?;
        Ok(())
    }
}

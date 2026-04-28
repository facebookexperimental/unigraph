// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;

use crate::UnigraphCLIContext;

/// Compute frame-type statistics for a timeline and print them as JSON.
///
/// Groups frames by type and time window, showing counts and sizes for
/// each combination. Useful for understanding storage patterns and
/// identifying timelines with unusual frame distributions.
#[derive(Parser, Debug)]
pub struct TimelinesStats {
    /// Timeline ID to collect stats for
    timeline_id: String,
}

impl TimelinesStats {
    pub fn new(timeline_id: String) -> Self {
        Self { timeline_id }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let output = crate::stats::run_timeline_stats(&self.timeline_id, &ctx.db, task).await?;
        let json = serde_json::to_string_pretty(&output)?;
        ctx.println_after_done(&json)?;
        Ok(())
    }
}

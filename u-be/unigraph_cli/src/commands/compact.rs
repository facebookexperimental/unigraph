// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;
use unigraph_storage_core::TimelineID;

use crate::parse_timestamp;

#[derive(Parser)]
pub struct Compact {
    /// Timeline ID to compact
    #[arg(long)]
    timeline_id: String,

    /// Start of the time range (RFC 3339, e.g. 2025-01-01T00:00:00Z). Defaults to beginning of time.
    #[arg(long)]
    start: Option<String>,

    /// End of the time range (RFC 3339, e.g. 2025-12-31T23:59:59Z). Defaults to now.
    #[arg(long)]
    end: Option<String>,
}

impl UnigraphCLISubcommand for Compact {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let start = parse_timestamp(self.start.as_deref())?;
        let end = parse_timestamp(self.end.as_deref())?;

        let timeline_id = TimelineID(self.timeline_id.clone());
        let converted = ctx.db.graph.compact(&timeline_id, start, end, task).await?;

        match converted {
            0 => ctx.println_after_done("Nothing to compact.")?,
            n => ctx.println_after_done(&format!("Compacted {n} frame(s) from Full to Delta."))?,
        }

        Ok(())
    }
}

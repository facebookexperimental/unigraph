// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;

use crate::UnigraphCLIContext;

/// List all timelines in the database.
///
/// Prints one timeline ID per line. Use `--frames` to also show the number
/// of frames stored for each timeline (requires an extra query per timeline).
#[derive(Parser, Debug)]
pub struct TimelinesList {
    /// Also show frame counts for each timeline (may be slow)
    #[arg(long)]
    frames: bool,
}

impl TimelinesList {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let timelines = ctx.db.timelines.list(task).await?;
        if timelines.is_empty() {
            ctx.eprintln_after_done("No timelines found in database.")?;
            return Ok(());
        }
        for tl in &timelines {
            if self.frames {
                let frames = ctx.db.frames.list(tl, task).await?;
                ctx.println_after_done(&format!("{} ({} frames)", tl.0, frames.len()))?;
            } else {
                ctx.println_after_done(&tl.0)?;
            }
        }
        Ok(())
    }
}

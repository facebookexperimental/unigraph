// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use clap::Parser;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphKey;

use crate::UnigraphCLIContext;

/// Fetch the error message from an error frame.
///
/// Looks up the frame by graph key, verifies it is an error frame, and
/// prints the stored error messages. Fails if the frame does not exist
/// or is not an error frame.
///
/// Examples:
///
/// ```sh
/// unigraph graph get-error my_timeline~42
/// ```
#[derive(Parser, Debug)]
pub struct GraphGetError {
    /// Graph key (`timeline_id~graph_id`)
    key: String,
}

impl GraphGetError {
    pub fn new(key: String) -> Self {
        Self { key }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let graph_key: GraphKey = self.key.parse().context("Failed to parse graph key")?;

        let frame = ctx
            .db
            .frames
            .get(&graph_key, false, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Frame '{}' not found", self.key))?;

        anyhow::ensure!(
            frame.frame_type == FrameType::Error,
            "Frame '{}' is not an error frame (type: {})",
            self.key,
            frame.frame_type,
        );

        let errors = ctx
            .db
            .graph
            .fetch_errors(&graph_key, task)
            .await
            .context("Failed to fetch errors")?;

        if errors.is_empty() {
            ctx.eprintln_after_done("Error frame has no error messages.")?;
            return Ok(());
        }

        for err in &errors {
            ctx.println_after_done(&format!("[{}] {}", err.timestamp.to_rfc3339(), err.message))?;
        }

        task.data("error_count", errors.len());
        Ok(())
    }
}

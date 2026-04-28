// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Create a new timeline from a JSON configuration file.
///
/// The config file must contain a valid `TimelineConfig` JSON object.
/// Minimal example:
///
/// ```json
/// { "schema": { "AdjacentDeltas": {} } }
/// ```
///
/// Full example with all fields:
///
/// ```json
/// {
///     "schema": { "AdjacentDeltas": {} },
///     "external_id_namespace": "my-repo/git",
///     "blob_storage": "External",
///     "store_metric_history": true
/// }
/// ```
#[derive(Parser, Debug)]
pub struct TimelinesPut {
    /// Timeline ID to create
    timeline_id: String,

    /// Path to a JSON file containing the TimelineConfig
    #[arg(long)]
    config_path: PathBuf,
}

impl TimelinesPut {
    pub fn new(timeline_id: String, config_path: PathBuf) -> Self {
        Self {
            timeline_id,
            config_path,
        }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let json = std::fs::read_to_string(&self.config_path).with_context(|| {
            format!("Failed to read config file: {}", self.config_path.display())
        })?;
        let config: unigraph_storage_core::TimelineConfig =
            serde_json::from_str(&json).context("Failed to parse TimelineConfig JSON")?;
        let timeline_id = TimelineID(self.timeline_id.clone());

        ctx.db.timelines.create(&timeline_id, &config, task).await?;
        ctx.eprintln_after_done(&format!("Created timeline '{}'", self.timeline_id))?;
        Ok(())
    }
}

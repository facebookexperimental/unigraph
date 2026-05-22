// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_storage_core::BlobStorageMode;
use unigraph_storage_core::FullOrDeltaConfig;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;

use crate::UnigraphCLIContext;

/// Create a new timeline, optionally from a JSON configuration file.
///
/// If `--config-path` is omitted, a default config with `AdjacentDeltas`
/// schema and inline blob storage is used.
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

    /// Path to a JSON file containing the TimelineConfig.
    /// If omitted, uses a default config with AdjacentDeltas schema.
    #[arg(long)]
    config_path: Option<PathBuf>,
}

impl TimelinesPut {
    pub fn new(timeline_id: String, config_path: Option<PathBuf>) -> Self {
        Self {
            timeline_id,
            config_path,
        }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let config = parse_config(&self.config_path)?;
        let timeline_id = TimelineID(self.timeline_id.clone());

        ctx.db.timelines.create(&timeline_id, &config, task).await?;
        ctx.eprintln_after_done(&format!("Created timeline '{}'", self.timeline_id))?;
        Ok(())
    }
}

fn parse_config(config_path: &Option<PathBuf>) -> anyhow::Result<TimelineConfig> {
    match config_path {
        Some(path) => {
            let json = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            serde_json::from_str(&json).context("Failed to parse TimelineConfig JSON")
        }
        None => Ok(TimelineConfig {
            schema: TimelineSchema::FullOrDelta(FullOrDeltaConfig {}),
            external_id_namespace: None,
            blob_storage: BlobStorageMode::Inline,
            store_metric_history: None,
        }),
    }
}

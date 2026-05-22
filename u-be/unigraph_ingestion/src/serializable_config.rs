// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

/// Root config: the `ingestion.json` file is a `Vec<IngestionConfig>`.
///
/// Entries are processed in array order, which matters when one entry
/// depends on another (e.g. `AnotherTimeline` references a timeline
/// produced by an earlier `Git` entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionConfig {
    pub source: IngestionSourceConfig,
    pub timelines: Vec<TimelineBuilderEntry>,
}

/// Source system for ingestion (externally tagged).
///
/// Serializes as `{"Git": {"repo_path": "~/p/unigraph", "main_branch": "main"}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngestionSourceConfig {
    Git {
        repo_path: PathBuf,
        main_branch: String,
    },
}

/// A single timeline + builder pair within an ingestion config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBuilderEntry {
    pub timeline_id: String,
    pub builder: GraphBuilderConfig,
}

/// Graph builder configuration (externally tagged).
///
/// Serializes as `{"Cargo": {"manifest_path": "Cargo.toml"}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphBuilderConfig {
    Cargo {
        manifest_path: String,
        #[serde(default)]
        collect_timings: bool,
        #[serde(default)]
        collect_sizes: bool,
    },
}

/// Load ingestion configs from a JSON file.
pub fn load_ingestion_configs(path: &Path) -> Result<Vec<IngestionConfig>> {
    let contents = std::fs::read_to_string(path).context("failed to read ingestion config file")?;
    serde_json::from_str(&contents).context("failed to parse ingestion config file")
}

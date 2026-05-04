// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::MapGraph;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Timestamp;

use crate::UnigraphCLIContext;

/// Store a graph JSON file as a frame in a timeline.
///
/// Reads a graph from a JSON file (auto-detects MapGraph and
/// ArrayGraphSerializable formats), and stores it under the given
/// graph key. Defaults to the current timestamp unless `--timestamp`
/// is provided.
///
/// Examples:
///
/// ```sh
/// # Store with current timestamp
/// unigraph graph put --json /tmp/graph.json --graph-key my_timeline~1
///
/// # Store with explicit timestamp
/// unigraph graph put --json /tmp/graph.json --graph-key my_timeline~1 \
///     --timestamp '2025-01-01T00:00:00Z'
///
/// # Store as delta from another graph
/// unigraph graph put --json /tmp/graph.json --graph-key my_timeline~2 \
///     --delta-from my_timeline~1
/// ```
#[derive(Parser, Debug)]
pub struct GraphPut {
    /// Path to a JSON file containing the graph (MapGraph or ArrayGraphSerializable)
    #[arg(long = "json")]
    json_path: PathBuf,

    /// Graph key in the format `timeline_id~graph_id`
    #[arg(long)]
    graph_key: String,

    /// Timestamp in RFC 3339 format. Defaults to now
    #[arg(long)]
    timestamp: Option<String>,

    /// Store as a delta from this base graph key (`timeline_id~graph_id`)
    #[arg(long)]
    delta_from: Option<String>,
}

impl GraphPut {
    pub fn new(json_path: PathBuf, graph_key: String) -> Self {
        Self {
            json_path,
            graph_key,
            timestamp: None,
            delta_from: None,
        }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let ags = parse_graph_file(&self.json_path, task)?;

        let graph_key: GraphKey = self
            .graph_key
            .parse()
            .context("Failed to parse --graph-key")?;

        let timestamp = match &self.timestamp {
            Some(ts) => ts
                .parse::<Timestamp>()
                .context("Failed to parse --timestamp")?,
            None => Timestamp::now(),
        };

        let key = GraphTimeKey {
            timeline_id: graph_key.timeline_id.clone(),
            timestamp,
            graph_id: graph_key.graph_id,
        };

        if let Some(delta_from_str) = &self.delta_from {
            let from_key: GraphKey = delta_from_str
                .parse()
                .context("Failed to parse --delta-from")?;
            ctx.db
                .graph
                .store_as_delta_from(&key, &ags, &from_key, task)
                .await
                .context("Failed to store graph as delta")?;
            ctx.eprintln_after_done(&format!(
                "Stored graph '{}' as delta from '{}'",
                graph_key, from_key
            ))?;
        } else {
            ctx.db
                .graph
                .store(&key, &ags, None, task)
                .await
                .context("Failed to store graph")?;
            ctx.eprintln_after_done(&format!("Stored graph '{}'", graph_key))?;
        }

        task.data("graph_key", &self.graph_key);
        Ok(())
    }
}

pub fn parse_graph_file(path: &PathBuf, task: &ll::Task) -> anyhow::Result<ArrayGraphSerializable> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read graph file: {}", path.display()))?;
    task.data("file_size_bytes", bytes.len());

    if let Ok(ags) = ArrayGraphSerializable::from_json_bytes(&bytes) {
        return Ok(ags);
    }

    let map_graph = MapGraph::from_json_bytes(&bytes)
        .context("Failed to parse graph file as MapGraph or ArrayGraphSerializable")?;
    map_graph
        .to_array_graph_serializable()
        .context("Failed to convert MapGraph to ArrayGraphSerializable")
}

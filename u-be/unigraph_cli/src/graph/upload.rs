// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_core::GraphID;
use unigraph_core::GraphTimeKey;
use unigraph_core::Timestamp;
use unigraph_storage_core::AdjacentDeltasConfig;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;

use crate::UnigraphCLIContext;
use crate::graph::put::parse_graph_file;

const SINKHOLE_TIMELINE: &str = "sinkhole";

/// Quick-upload a graph JSON file to the default "sinkhole" timeline.
///
/// Creates the timeline if it doesn't exist, assigns a unique ID
/// based on the current timestamp, and prints the result as JSON.
///
/// Examples:
///
/// ```sh
/// unigraph graph upload /tmp/graph.json
/// unigraph graph upload /tmp/before.json /tmp/after.json
/// ```
#[derive(Parser, Debug)]
pub struct GraphUpload {
    /// Path(s) to graph JSON files
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

impl GraphUpload {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        ensure_timeline_exists(ctx, task).await?;

        for file in &self.files {
            let ags = parse_graph_file(file, task)?;
            let timestamp = Timestamp::now();
            let graph_id = GraphID(ctx.db.utility.gen_uniq_id(task).await?);

            let tid = TimelineID(SINKHOLE_TIMELINE.to_string());
            let key = GraphTimeKey {
                timeline_id: tid,
                timestamp,
                graph_id,
            };

            ctx.db
                .graph
                .store(&key, &ags, None, task)
                .await
                .context("Failed to store graph")?;

            let graph_key = format!("{SINKHOLE_TIMELINE}~{}", graph_id.0);
            let ag = ags.into_array_graph(task)?;
            let stats = ag.stats();
            let result = serde_json::json!({
                "graph_key": graph_key,
                "timeline": SINKHOLE_TIMELINE,
                "graph_id": graph_id.0,
                "file": file.display().to_string(),
                "stats": {
                    "nodes": stats.num_all_nodes,
                    "edges": stats.num_all_edges,
                },
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }

        Ok(())
    }
}

async fn ensure_timeline_exists(ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
    let tid = TimelineID(SINKHOLE_TIMELINE.to_string());
    let existing = ctx.db.timelines.list(task).await?;
    if existing.contains(&tid) {
        return Ok(());
    }
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };
    ctx.db
        .timelines
        .create(&tid, &config, task)
        .await
        .context("Failed to create sinkhole timeline")?;
    Ok(())
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::io::BufWriter;

use anyhow::Context;
use clap::Parser;
use unigraph_storage_core::GraphKeyOrTimelineID;

use crate::UnigraphCLIContext;

/// Fetch a graph and save it as JSON.
///
/// Accepts a full graph key (`timeline_id~graph_id`) to fetch a specific
/// graph, or just a timeline ID to fetch the latest. By default writes
/// MapGraph JSON to a temporary file. Use `--array-graph` to skip the
/// MapGraph conversion, or `--stdout` to print to stdout.
///
/// Examples:
///
/// ```sh
/// # Fetch a specific graph
/// unigraph graph get my_timeline~123
///
/// # Fetch the latest graph from a timeline
/// unigraph graph get my_timeline
///
/// # Print to stdout
/// unigraph graph get my_timeline~123 --stdout
///
/// # Keep the compact array-graph format
/// unigraph graph get my_timeline~123 --array-graph
/// ```
#[derive(Parser, Debug)]
pub struct GraphGet {
    /// Graph key (`timeline_id~graph_id`) or timeline ID for latest
    key: String,

    /// Print JSON to stdout instead of writing to a file
    #[arg(long)]
    stdout: bool,

    /// Serialize as ArrayGraphSerializable without converting to MapGraph
    #[arg(long)]
    array_graph: bool,
}

impl GraphGet {
    pub fn new(key: String, stdout: bool, array_graph: bool) -> Self {
        Self {
            key,
            stdout,
            array_graph,
        }
    }

    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let parsed: GraphKeyOrTimelineID = self
            .key
            .parse()
            .context("Failed to parse graph key or timeline ID")?;

        let (resolved_key, ags) = match parsed {
            GraphKeyOrTimelineID::GraphKey(ref key) => {
                let ags = ctx
                    .db
                    .graph
                    .fetch(key, task)
                    .await
                    .context("Failed to fetch graph")?;
                (key.clone(), ags)
            }
            GraphKeyOrTimelineID::TimelineID(ref timeline_id) => {
                let (key, ags) = ctx
                    .db
                    .graph
                    .fetch_latest(timeline_id, task)
                    .await
                    .context("Failed to fetch latest graph")?;
                ctx.eprintln_after_done(&format!("Resolved to graph '{}'", key))?;
                (key, ags)
            }
        };

        task.data("graph_key", resolved_key.to_string());

        if self.array_graph {
            self.write_output(ctx, &resolved_key, &ags, task)
        } else {
            let map_graph = ags
                .into_array_graph(task)?
                .to_map_graph()
                .context("Failed to convert to MapGraph")?;
            self.write_output(ctx, &resolved_key, &map_graph, task)
        }
    }

    #[ll::task(sync)]
    fn write_output(
        &self,
        ctx: &UnigraphCLIContext,
        key: &unigraph_storage_core::GraphKey,
        value: &impl serde::Serialize,
        task: &ll::Task,
    ) -> anyhow::Result<()> {
        if self.stdout {
            let json =
                serde_json::to_string_pretty(value).context("Failed to serialize graph to JSON")?;
            ctx.println_after_done(&json)?;
        } else {
            let dir = std::env::temp_dir().join("unigraph");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{}_{}.json", key, std::process::id()));
            let file = std::fs::File::create(&path).context("Failed to create output file")?;
            let writer = BufWriter::new(file);
            serde_json::to_writer_pretty(writer, value)
                .context("Failed to serialize JSON to file")?;
            let size_bytes = std::fs::metadata(&path)?.len();
            let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
            task.data("output_path", path.display().to_string());
            let summary = serde_json::json!({
                "timeline_id": &key.timeline_id.0,
                "graph_id": &key.graph_id.0,
                "graph_key": key.to_string(),
                "size_mb": format!("{:.2}", size_mb),
                "path": path.display().to_string(),
            });
            ctx.eprintln_after_done(&serde_json::to_string_pretty(&summary)?)?;
        }
        Ok(())
    }
}

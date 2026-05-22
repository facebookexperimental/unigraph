// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::BufWriter;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_core::GraphHandle;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::config_query::TraversalOverride;
use unigraph_storage_core::GraphKey;

use crate::UnigraphCLIContext;

/// Fetch a graph and save it as JSON.
///
/// Accepts a graph handle in one of three forms:
/// - `gqc_{hash}` — GQC key (applies embedded traversal config and entry points)
/// - `timeline_id~graph_id` — specific graph snapshot
/// - `timeline_id` — latest graph from a timeline
///
/// Optional `--roots` and `--traversal` override the GQC defaults (or apply
/// filtering/traversal to a plain graph handle).
///
/// By default writes MapGraph JSON to a temporary file. Use `--array-graph`
/// to keep the compact array format, or `--stdout` to print to stdout.
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
/// # Fetch via GQC key (includes traversal config + entry points)
/// unigraph graph get gqc_1a2b3c4d5e6f7890
///
/// # Override roots (repeatable)
/// unigraph graph get my_timeline --roots nodeA --roots nodeB
///
/// # Roots from a JSON file (array or object-with-keys)
/// unigraph graph get my_timeline --roots-json roots.json
///
/// # Override traversal config
/// unigraph graph get my_timeline --traversal tvc_1a2b3c4d5e6f7890
///
/// # Print to stdout
/// unigraph graph get my_timeline~123 --stdout
///
/// # Keep the compact array-graph format
/// unigraph graph get my_timeline~123 --array-graph
/// ```
#[derive(Parser, Debug)]
pub struct GraphGet {
    /// Graph handle: `gqc_{hash}`, `timeline_id~graph_id`, or `timeline_id`
    handle: String,

    /// Root node names for subgraph extraction (repeatable)
    #[arg(long)]
    roots: Option<Vec<String>>,

    /// File containing root node names as JSON.
    /// Accepts either a JSON array `["A", "B"]` or a JSON object
    /// `{"A": ..., "B": ...}` (only keys are used). Merged with `--roots`.
    #[arg(long)]
    roots_json: Option<PathBuf>,

    /// Traversal config key (`tvc_{hash}`) to override graph traversal
    #[arg(long)]
    traversal: Option<String>,

    /// Print JSON to stdout instead of writing to a file
    #[arg(long)]
    stdout: bool,

    /// Serialize as ArrayGraphSerializable without converting to MapGraph
    #[arg(long)]
    array_graph: bool,
}

impl GraphGet {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let gqc = self.build_query_config()?;
        let (key, ag) = ctx.db.resolve_graph_query_config(&gqc, task).await?;
        task.data("graph_key", key.to_string());

        let graph_nodes = ag.nodes_len();
        let graph_edges = ag.data.edges.edges_len();

        if self.array_graph {
            let ags = ag.into_serializable();
            self.write_output(ctx, &key, &ags, graph_nodes, graph_edges, task)
        } else {
            let map_graph = ag.to_map_graph().context("Failed to convert to MapGraph")?;
            self.write_output(ctx, &key, &map_graph, graph_nodes, graph_edges, task)
        }
    }
}

// -- Private helpers ----------------------------------------------------------

impl GraphGet {
    fn build_query_config(&self) -> anyhow::Result<GraphQueryConfig> {
        let handle: GraphHandle = self
            .handle
            .parse()
            .context("Failed to parse graph handle")?;

        let roots = self.collect_roots()?;

        let traversal = self
            .traversal
            .as_ref()
            .map(|t| t.parse())
            .transpose()
            .context("Failed to parse traversal config key")?
            .map(TraversalOverride::Key);

        Ok(GraphQueryConfig {
            handle,
            roots,
            traversal,
        })
    }

    fn collect_roots(&self) -> anyhow::Result<Option<BTreeSet<String>>> {
        let roots_from_file = self.parse_roots_json()?;

        let all_roots: BTreeSet<String> = roots_from_file
            .into_iter()
            .chain(
                self.roots
                    .as_ref()
                    .into_iter()
                    .flat_map(|r| r.iter().cloned()),
            )
            .collect();

        Ok(if all_roots.is_empty() {
            None
        } else {
            Some(all_roots)
        })
    }

    fn parse_roots_json(&self) -> anyhow::Result<Vec<String>> {
        let path = match &self.roots_json {
            Some(p) => p,
            None => return Ok(vec![]),
        };

        let json = std::fs::read_to_string(path).context("Failed to read --roots-json file")?;

        serde_json::from_str::<Vec<String>>(&json).or_else(|_| {
            serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&json)
                .map(|map| map.into_keys().collect())
                .context("--roots-json must be a JSON array or a JSON object")
        })
    }

    fn write_output(
        &self,
        ctx: &UnigraphCLIContext,
        key: &GraphKey,
        value: &impl serde::Serialize,
        graph_nodes: usize,
        graph_edges: usize,
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
                "graph_nodes": graph_nodes,
                "graph_edges": graph_edges,
                "size_mb": format!("{:.2}", size_mb),
                "path": path.display().to_string(),
            });
            ctx.eprintln_after_done(&serde_json::to_string_pretty(&summary)?)?;
        }
        Ok(())
    }
}

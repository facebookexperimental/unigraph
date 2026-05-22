// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "unigraph_turbopack",
    about = "Convert Turbopack analyze data to Unigraph JSON"
)]
struct Cli {
    /// Path to .next/diagnostics/analyze/data/ directory
    data_dir: PathBuf,

    /// Output JSON file path
    #[arg(short, long, default_value = "turbopack-graph.json")]
    output: PathBuf,

    /// Keep tree-shaking fragments as separate nodes
    #[arg(long)]
    fragments: bool,

    /// Assign size metrics to all layers (RSC, SSR, client).
    /// By default, only app-client and layerless nodes get sizes.
    #[arg(long)]
    all_layer_sizes: bool,

    /// Pretty-print the JSON output
    #[arg(long)]
    pretty: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let opts = unigraph_turbopack::Options {
        fragments: cli.fragments,
        all_layer_sizes: cli.all_layer_sizes,
    };
    let graph = unigraph_turbopack::build_map_graph(&cli.data_dir, &opts)?;
    write_output(&graph, &cli.output, cli.pretty)?;
    print_summary(&graph);
    Ok(())
}

fn write_output(graph: &unigraph_core::MapGraph, output: &PathBuf, pretty: bool) -> Result<()> {
    let json = if pretty {
        serde_json::to_string_pretty(graph)?
    } else {
        serde_json::to_string(graph)?
    };
    fs::write(output, &json).with_context(|| format!("writing {}", output.display()))?;
    eprintln!("Wrote {} bytes to {}", json.len(), output.display());
    Ok(())
}

fn print_summary(graph: &unigraph_core::MapGraph) {
    let node_count = graph.nodes.len();
    let edge_count: usize = graph
        .nodes
        .values()
        .map(|n| {
            let directed = n.edges_directed.as_ref().map_or(0, |e| e.len());
            let tagged: usize = n
                .edges_tagged
                .as_ref()
                .map_or(0, |t| t.values().map(|v| v.len()).sum());
            directed + tagged
        })
        .sum();
    let with_size = graph.nodes.values().filter(|n| n.metrics.is_some()).count();
    eprintln!("{node_count} nodes, {edge_count} edges, {with_size} nodes with size data");
}

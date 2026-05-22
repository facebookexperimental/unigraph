// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;

#[derive(Parser)]
pub struct ImpactAnalysisCmd {
    /// Path to the input graph JSON file (MapGraph format)
    file_path: PathBuf,

    /// Only analyze nodes with at most this many parents
    #[arg(long)]
    max_parents: Option<usize>,
}

impl UnigraphCLISubcommand for ImpactAnalysisCmd {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let ag = load_graph_from_file(&self.file_path, task)?;
        let node_count = ag.nodes_len();

        ctx.eprintln_after_done(&format!(
            "Running impact analysis on {} ({} nodes)...",
            self.file_path.display(),
            node_count
        ))?;

        let analysis = unigraph_app::impact_analysis::ImpactAnalysis {
            ag,
            max_parents: self.max_parents,
        };
        let ag = analysis.run(task)?;

        let output_path = make_output_path(&self.file_path, "_impact_analysis");
        let map_graph = ag.to_map_graph()?;
        let json = serde_json::to_string_pretty(&map_graph)?;
        std::fs::write(&output_path, &json).context("failed to write output file")?;

        ctx.eprintln_after_done(&format!("Wrote {}", output_path.display()))?;
        Ok(())
    }
}

fn load_graph_from_file(path: &Path, task: &ll::Task) -> anyhow::Result<unigraph_core::ArrayGraph> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let map_graph = unigraph_core::types::MapGraph::from_json(&json)?;
    map_graph.to_array_graph(task)
}

fn make_output_path(input: &Path, suffix: &str) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let ext = input
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    input.with_file_name(format!("{stem}{suffix}{ext}"))
}

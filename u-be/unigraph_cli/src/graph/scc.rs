// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_core::SccGraph;
use unigraph_storage_core::GraphKey;

use crate::UnigraphCLIContext;
use crate::graph::SubgraphArgs;

/// Condense a graph into its tree of strongly connected components (SCCs).
///
/// Computes SCCs (Tarjan) over the traversed subgraph, then builds a new graph
/// where each SCC is a single node:
/// - A singleton SCC stays a regular node (same name, metrics, labels).
/// - A cyclic SCC (>1 node) becomes a synthetic `SCC #N` node whose metrics are
///   the sum of its members' metrics, plus a `scc_node_count` metric.
///
/// Edges pointing *into* a multi-node SCC become dynamic edges (type `scc`,
/// name `in`) with one branch per entered component — so you can see how each
/// cluster is reached. The result is a DAG/tree of clusters: the big cycles and
/// how they connect.
///
/// The handle, `--roots`, and `--traversal` options behave exactly as in
/// `unigraph graph get`.
///
/// Writes two files to a temp directory (paths printed at the end):
/// - the condensed graph as MapGraph JSON
/// - a `{SCC #N: [member nodes...]}` map for the multi-node SCCs
///
/// Examples:
///
/// ```sh
/// # Condense a timeline's latest graph into its SCC tree
/// unigraph graph scc my_timeline
///
/// # With explicit roots / traversal override
/// unigraph graph scc my_timeline --roots nodeA --traversal tvc_1a2b3c4d5e6f7890
/// ```
#[derive(Parser, Debug)]
pub struct GraphScc {
    #[command(flatten)]
    subgraph: SubgraphArgs,
}

impl GraphScc {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let (key, ag) = self.subgraph.fetch(ctx, task).await?;
        task.data("graph_key", key.to_string());

        let scc = ag.to_scc_graph().context("Failed to build SCC graph")?;
        task.data("scc_nodes", scc.graph.nodes.len());
        task.data("multi_node_sccs", scc.members.len());

        self.write_output(ctx, &key, &scc, task)
    }
}

// -- Private helpers ----------------------------------------------------------

impl GraphScc {
    fn write_output(
        &self,
        ctx: &UnigraphCLIContext,
        key: &GraphKey,
        scc: &SccGraph,
        task: &ll::Task,
    ) -> anyhow::Result<()> {
        let graph_path = write_json(key, "scc_graph", &scc.graph)?;
        let members_path = write_json(key, "scc_members", &scc.members)?;

        task.data("scc_graph_path", graph_path.display().to_string());
        task.data("scc_members_path", members_path.display().to_string());

        ctx.println_after_done(&format!("SCC graph:   {}", graph_path.display()))?;
        ctx.println_after_done(&format!("SCC members: {}", members_path.display()))?;
        ctx.eprintln_after_done(&format!(
            "{} SCC node(s), {} multi-node SCC(s)",
            scc.graph.nodes.len(),
            scc.members.len()
        ))?;
        Ok(())
    }
}

/// Serialize `value` as pretty JSON into `<tmp>/unigraph/{key}_{pid}_{label}.json`.
fn write_json(
    key: &GraphKey,
    label: &str,
    value: &impl serde::Serialize,
) -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join("unigraph");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}_{}_{}.json", key, std::process::id(), label));
    write_pretty(&path, value)?;
    Ok(path)
}

fn write_pretty(path: &Path, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let file = std::fs::File::create(path).context("Failed to create output file")?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, value).context("Failed to serialize JSON to file")
}

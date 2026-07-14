// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use unigraph_core::ArrayGraph;
use unigraph_core::NodeIDX;
use unigraph_core::min_cut;

use crate::UnigraphCLIContext;
use crate::graph::SubgraphArgs;
use crate::graph::subgraph_args::parse_names_json;

/// Find the minimum set of edges to cut so a feature disappears from the graph.
///
/// Given a set of modules (a feature), computes the minimum-cardinality set of
/// dependency edges to remove so that none of those modules stay reachable from
/// the graph's entry points. After cutting them, the feature — plus anything
/// kept alive only by it — becomes dead code you can delete.
///
/// This is a max-flow / min-cut over the traversed subgraph. Unlike a dominator
/// tree, it handles features reachable through many independent paths. The cut
/// returned is the one nearest the feature (the import edges pointing into it).
///
/// The handle, `--roots`, and `--traversal` options behave exactly as in
/// `unigraph graph get`.
///
/// Examples:
///
/// ```sh
/// # Minimum edges to cut to remove two modules
/// unigraph graph cut my_timeline --module feature/a --module feature/b
///
/// # Modules from a JSON file (array or object-with-keys)
/// unigraph graph cut my_timeline --modules-json feature.json
///
/// # As JSON
/// unigraph graph cut my_timeline --module feature/a --json
/// ```
#[derive(Parser, Debug)]
pub struct GraphCut {
    #[command(flatten)]
    subgraph: SubgraphArgs,

    /// Module names to cut out of the graph (repeatable)
    #[arg(long = "module", num_args = 1)]
    modules: Vec<String>,

    /// File containing module names as JSON.
    /// Accepts either a JSON array `["A", "B"]` or a JSON object
    /// `{"A": ..., "B": ...}` (only keys are used). Merged with `--module`.
    #[arg(long)]
    modules_json: Option<PathBuf>,

    /// Print the cut as JSON instead of a plain edge list
    #[arg(long)]
    json: bool,
}

impl GraphCut {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let (key, ag) = self.subgraph.fetch(ctx, task).await?;
        task.data("graph_key", key.to_string());

        let sinks = self.resolve_modules(&ag)?;
        let sources = ag.determine_entrypoints();

        let result = min_cut(&ag, &sources, &sinks);
        let cut: Vec<(String, String)> = result
            .cut_edges
            .iter()
            .map(|&(from, to)| {
                (
                    ag.idx_to_name(from).to_owned(),
                    ag.idx_to_name(to).to_owned(),
                )
            })
            .collect();

        task.data("cut_edges", cut.len());
        self.write_output(ctx, &cut, result.has_uncuttable_sink)
    }
}

// -- Private helpers ----------------------------------------------------------

impl GraphCut {
    fn resolve_modules(&self, ag: &ArrayGraph) -> anyhow::Result<Vec<NodeIDX>> {
        let names = self.collect_modules()?;
        if names.is_empty() {
            bail!("no modules given — pass --module <name> (repeatable) or --modules-json <file>");
        }

        let mut sinks = Vec::with_capacity(names.len());
        let mut unknown = Vec::new();
        for name in &names {
            match ag.data.node_names_ordered.name_to_idx_log(name) {
                Some(idx) => sinks.push(idx),
                None => unknown.push(name.clone()),
            }
        }
        if !unknown.is_empty() {
            bail!("modules not found in graph: {}", unknown.join(", "));
        }
        Ok(sinks)
    }

    fn collect_modules(&self) -> anyhow::Result<Vec<String>> {
        let from_file = match &self.modules_json {
            Some(path) => parse_names_json(path).context("Failed to read --modules-json file")?,
            None => vec![],
        };
        let all: BTreeSet<String> = from_file
            .into_iter()
            .chain(self.modules.iter().cloned())
            .collect();
        Ok(all.into_iter().collect())
    }

    fn write_output(
        &self,
        ctx: &UnigraphCLIContext,
        cut: &[(String, String)],
        has_uncuttable_sink: bool,
    ) -> anyhow::Result<()> {
        if self.json {
            let edges: Vec<_> = cut
                .iter()
                .map(|(from, to)| serde_json::json!({ "from": from, "to": to }))
                .collect();
            let payload = serde_json::json!({
                "cut_edges": edges,
                "has_uncuttable_sink": has_uncuttable_sink,
            });
            ctx.println_after_done(&serde_json::to_string_pretty(&payload)?)?;
        } else {
            let body = if cut.is_empty() {
                "(no cut needed — modules already unreachable from entry points)".to_owned()
            } else {
                cut.iter()
                    .map(|(from, to)| format!("{from} -> {to}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            ctx.println_after_done(&body)?;
        }

        if has_uncuttable_sink {
            ctx.eprintln_after_done(
                "warning: some modules are entry points and cannot be cut off by removing edges — delete them directly",
            )?;
        }
        ctx.eprintln_after_done(&format!("{} edge(s) to cut", cut.len()))?;
        Ok(())
    }
}

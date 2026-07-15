// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;
use std::path::Path;
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

/// Separator for edges given as `from->to` on the CLI / in JSON.
const EDGE_SEP: &str = "->";

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
///
/// # Keep specific edges — find the smallest cut that avoids them
/// unigraph graph cut my_timeline --module feature/a --protect 'x->feature/a'
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

    /// Edges to keep — never cut these. The result is the smallest cut that
    /// avoids them (same size or larger, never smaller). Format: `from->to`
    /// (repeatable).
    #[arg(long = "protect", num_args = 1)]
    protect: Vec<String>,

    /// File of protected edges as JSON: an array of `"from->to"` strings or of
    /// `{"from": ..., "to": ...}` objects (e.g. the `cut_edges` from `--json`).
    /// Merged with `--protect`.
    #[arg(long)]
    protect_json: Option<PathBuf>,

    /// Print the cut as JSON instead of a plain edge list
    #[arg(long)]
    json: bool,
}

impl GraphCut {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let (key, ag) = self.subgraph.fetch(ctx, task).await?;
        task.data("graph_key", key.to_string());

        let sinks = self.resolve_modules(&ag)?;
        let protected = self.resolve_protected(&ag)?;
        let sources = ag.determine_entrypoints();

        let result = min_cut(&ag, &sources, &sinks, &protected);
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
        task.data("blocked_by_protected", result.blocked_by_protected);
        self.write_output(
            ctx,
            &cut,
            result.has_uncuttable_sink,
            result.blocked_by_protected,
        )
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

    fn resolve_protected(&self, ag: &ArrayGraph) -> anyhow::Result<BTreeSet<(NodeIDX, NodeIDX)>> {
        let pairs = self.collect_protected_edges()?;

        let mut set = BTreeSet::new();
        let mut unknown = Vec::new();
        for (from, to) in &pairs {
            let from_idx = ag.data.node_names_ordered.name_to_idx_log(from);
            let to_idx = ag.data.node_names_ordered.name_to_idx_log(to);
            match (from_idx, to_idx) {
                (Some(f), Some(t)) => {
                    set.insert((f, t));
                }
                _ => unknown.push(format!("{from}{EDGE_SEP}{to}")),
            }
        }
        if !unknown.is_empty() {
            bail!(
                "protected edges reference nodes not in the graph: {}",
                unknown.join(", ")
            );
        }
        Ok(set)
    }

    fn collect_protected_edges(&self) -> anyhow::Result<Vec<(String, String)>> {
        let mut edges = Vec::new();
        for spec in &self.protect {
            edges.push(parse_edge_spec(spec)?);
        }
        if let Some(path) = &self.protect_json {
            edges.extend(parse_protected_json(path)?);
        }
        Ok(edges)
    }

    fn write_output(
        &self,
        ctx: &UnigraphCLIContext,
        cut: &[(String, String)],
        has_uncuttable_sink: bool,
        blocked_by_protected: bool,
    ) -> anyhow::Result<()> {
        if self.json {
            let edges: Vec<_> = cut
                .iter()
                .map(|(from, to)| serde_json::json!({ "from": from, "to": to }))
                .collect();
            let payload = serde_json::json!({
                "cut_edges": edges,
                "has_uncuttable_sink": has_uncuttable_sink,
                "blocked_by_protected": blocked_by_protected,
            });
            ctx.println_after_done(&serde_json::to_string_pretty(&payload)?)?;
        } else {
            let body = if blocked_by_protected {
                "(no cut possible — the feature is reachable from entry points only through protected edges; loosen --protect)".to_owned()
            } else if cut.is_empty() {
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
        if !blocked_by_protected {
            ctx.eprintln_after_done(&format!("{} edge(s) to cut", cut.len()))?;
        }
        Ok(())
    }
}

// -- Edge parsing -------------------------------------------------------------

/// Parse a single `from->to` edge spec.
fn parse_edge_spec(spec: &str) -> anyhow::Result<(String, String)> {
    let (from, to) = spec
        .split_once(EDGE_SEP)
        .with_context(|| format!("invalid edge `{spec}` — expected `from{EDGE_SEP}to`"))?;
    let (from, to) = (from.trim(), to.trim());
    if from.is_empty() || to.is_empty() {
        bail!("invalid edge `{spec}` — expected `from{EDGE_SEP}to`");
    }
    Ok((from.to_owned(), to.to_owned()))
}

/// Parse protected edges from JSON: either an array of `"from->to"` strings or
/// an array of `{"from": ..., "to": ...}` objects.
fn parse_protected_json(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    #[derive(serde::Deserialize)]
    struct EdgeObj {
        from: String,
        to: String,
    }

    let json = std::fs::read_to_string(path).context("Failed to read --protect-json file")?;

    if let Ok(specs) = serde_json::from_str::<Vec<String>>(&json) {
        return specs.iter().map(|s| parse_edge_spec(s)).collect();
    }
    let objs: Vec<EdgeObj> = serde_json::from_str(&json)
        .context("--protect-json must be an array of \"from->to\" strings or {from, to} objects")?;
    Ok(objs.into_iter().map(|e| (e.from, e.to)).collect())
}

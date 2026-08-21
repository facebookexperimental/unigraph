// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Minimum edge cut over a cached, fully-traversed graph.
//!
//! Shaped like the ExploreGraph RPC on purpose: a [`GraphQueryConfig`] picks the
//! graph out of the shared explore cache, so the usual interactive loop — cut,
//! protect one of the proposed edges, cut again — never re-fetches or
//! re-traverses the graph.
//!
//! ```text
//!   query ──► graph_cache.get_explored ──► Arc<ArrayGraph>
//!   sinks ─────────► names ──► NodeIDX ──┐
//!   protected_edges ► names ──► NodeIDX ─┴─► unigraph_core::min_cut ──► edges
//! ```
//!
//! All the real work is [`unigraph_core::min_cut`] (Dinic max-flow). This module
//! only translates between node names and `NodeIDX`, paginates, and renders.
//! Sources are the graph's entry points, matching the UI and the
//! `unigraph graph cut` CLI — narrow the starting set with `query.roots`.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::MinCut;
use unigraph_core::NodeIDX;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::min_cut;
use unigraph_rpc::RpcExec;

use crate::Unigraph;
use crate::rpc_req::ascii_table::trim_trailing_spaces;
use crate::rpc_req::ascii_table::write_cell;
use crate::rpc_req::ascii_table::write_separator;

// ── Types ────────────────────────────────────────────────────

/// An edge as a pair of node names.
///
/// The name-space counterpart of `unigraph_core::MinCutEdge`, which is
/// `NodeIDX`-based: indices are meaningless to an RPC caller, who never sees the
/// graph's index space.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct MinCutNamedEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct MinCutInput {
    /// The query config: which graph, optional roots, optional traversal.
    pub query: GraphQueryConfig,

    /// Nodes to sever from the graph's entry points — a whole feature, not just
    /// one module. One cut is computed for the entire set, which is what makes
    /// it minimal: severing a shared parent counts once, not once per sink.
    pub sinks: Vec<String>,

    /// Edges that must never be cut. The result is the smallest cut avoiding all
    /// of them — the same size or larger than the unconstrained cut, never
    /// smaller. Protecting every path to a sink makes the cut impossible, which
    /// surfaces as `blocked_by_protected`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_edges: Vec<MinCutNamedEdge>,

    /// Skip first N cut edges (for pagination). Defaults to 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,

    /// Maximum number of cut edges to return. Defaults to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// When true, populate the `ascii` field in the response with a
    /// human-readable rendering (optimized for agent / LLM consumption).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ascii: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct MinCutOutput {
    /// The edges to remove, sorted by name (paginated). Removing all of them
    /// makes every cuttable sink unreachable from every entry point. Empty when
    /// the sinks are already unreachable, or when `blocked_by_protected` is set.
    pub cut_edges: Vec<MinCutNamedEdge>,

    /// Total number of cut edges before offset/limit.
    pub total_cut_edges_count: usize,

    /// Sinks that are themselves entry points. No edge removal can make these
    /// unreachable — you have to delete the module. `cut_edges` covers only the
    /// remaining, cuttable sinks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncuttable_sinks: Vec<String>,

    /// True when the sinks hang off the entry points *only* through protected
    /// edges, so no cut avoiding them exists. `cut_edges` is then empty.
    pub blocked_by_protected: bool,

    /// Human-readable rendering of the result. Only populated when
    /// `include_ascii` is set to true in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<String>,
}

// ── Handler ──────────────────────────────────────────────────

const DEFAULT_LIMIT: usize = 100;

impl RpcExec<Unigraph> for MinCutInput {
    type Output = MinCutOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<MinCutOutput> {
        // Checked before the fetch — an empty request shouldn't pull a
        // multi-megabyte graph into the cache just to be rejected.
        if self.sinks.is_empty() {
            bail!("at least one sink node is required");
        }
        let ttl = Duration::from_mins(5);
        let ag = ctx.graph_cache.get_explored(&self.query, task, ttl).await?;
        let input = self;
        task.spawn("min_cut", |task| async move {
            tokio::task::spawn_blocking(move || compute_min_cut(ag, &input, &task))
                .await
                .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Sync core logic (runs in spawn_blocking) ────────────────────

fn compute_min_cut(
    ag: Arc<ArrayGraph>,
    input: &MinCutInput,
    task: &ll::Task,
) -> Result<MinCutOutput> {
    let sinks = resolve_sinks(&ag, &input.sinks)?;
    let protected = resolve_protected(&ag, &input.protected_edges)?;
    let sources = ag.determine_entrypoints();

    let cut = min_cut(&ag, &sources, &sinks, &protected);
    task.data("cut_edges", cut.cut_edges.len());
    task.data("blocked_by_protected", cut.blocked_by_protected);

    let all_cut_edges = name_edges(&ag, &cut.cut_edges);
    let total_cut_edges_count = all_cut_edges.len();
    let uncuttable_sinks = name_uncuttable_sinks(&ag, &cut, &sources, &sinks);

    let offset = input.offset.unwrap_or(0);
    let cut_edges = paginate(all_cut_edges, offset, input.limit.unwrap_or(DEFAULT_LIMIT));

    let mut output = MinCutOutput {
        cut_edges,
        total_cut_edges_count,
        uncuttable_sinks,
        blocked_by_protected: cut.blocked_by_protected,
        ascii: None,
    };
    if input.include_ascii.unwrap_or(false) {
        output.ascii = Some(render_ascii(input, &output));
    }
    Ok(output)
}

// ── Name ⇄ index resolution ─────────────────────────────────────

fn resolve_sinks(ag: &ArrayGraph, names: &[String]) -> Result<Vec<NodeIDX>> {
    let (found, unknown): (Vec<_>, Vec<_>) = names
        .iter()
        .map(|name| (name, resolve_node(ag, name)))
        .partition(|(_, idx)| idx.is_some());

    if !unknown.is_empty() {
        bail!("sink node(s) not found in graph: {}", join_names(&unknown));
    }
    Ok(found.into_iter().filter_map(|(_, idx)| idx).collect())
}

fn resolve_protected(
    ag: &ArrayGraph,
    edges: &[MinCutNamedEdge],
) -> Result<BTreeSet<(NodeIDX, NodeIDX)>> {
    let mut resolved = BTreeSet::new();
    let mut unknown = Vec::new();

    for edge in edges {
        match (resolve_node(ag, &edge.from), resolve_node(ag, &edge.to)) {
            (Some(from), Some(to)) => {
                resolved.insert((from, to));
            }
            _ => unknown.push(format_edge(&edge.from, &edge.to)),
        }
    }

    if !unknown.is_empty() {
        bail!(
            "protected edge(s) reference nodes not in graph: {}",
            unknown.join(", ")
        );
    }
    Ok(resolved)
}

fn resolve_node(ag: &ArrayGraph, name: &str) -> Option<NodeIDX> {
    ag.data.node_names_ordered.name_to_idx_log(name)
}

fn join_names(entries: &[(&String, Option<NodeIDX>)]) -> String {
    entries
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn name_edges(ag: &ArrayGraph, edges: &[(NodeIDX, NodeIDX)]) -> Vec<MinCutNamedEdge> {
    edges
        .iter()
        .map(|&(from, to)| MinCutNamedEdge {
            from: ag.idx_to_name(from).to_string(),
            to: ag.idx_to_name(to).to_string(),
        })
        .collect()
}

/// Which sinks were uncuttable. `min_cut` owns the *whether* — we only name the
/// culprits when it says there are some, so the two can't drift apart.
fn name_uncuttable_sinks(
    ag: &ArrayGraph,
    cut: &MinCut,
    sources: &[NodeIDX],
    sinks: &[NodeIDX],
) -> Vec<String> {
    if !cut.has_uncuttable_sink {
        return Vec::new();
    }
    let source_set: BTreeSet<NodeIDX> = sources.iter().copied().collect();
    sinks
        .iter()
        .filter(|idx| source_set.contains(idx))
        .map(|&idx| ag.idx_to_name(idx).to_string())
        .collect()
}

// ── Pagination ──────────────────────────────────────────────────

fn paginate(edges: Vec<MinCutNamedEdge>, offset: usize, limit: usize) -> Vec<MinCutNamedEdge> {
    edges.into_iter().skip(offset).take(limit).collect()
}

// ── ASCII rendering ─────────────────────────────────────────────

const BLOCKED_MESSAGE: &str = "(no cut possible — the sinks are reachable from the entry points only through protected edges)";
const NO_CUT_MESSAGE: &str =
    "(no cut needed — the sinks are already unreachable from the entry points)";
const FROM_HEADER: &str = "from";
const TO_HEADER: &str = "to";

/// Renders `output` (whose `ascii` field is still `None` — this is what fills
/// it), using `input` for the request echo and the page offset.
fn render_ascii(input: &MinCutInput, output: &MinCutOutput) -> String {
    let mut out = String::with_capacity(256);
    write_summary(&mut out, input);
    write_uncuttable_warning(&mut out, &output.uncuttable_sinks);

    if output.blocked_by_protected {
        out.push_str(BLOCKED_MESSAGE);
        return out;
    }
    if output.total_cut_edges_count > 0 {
        write_table(&mut out, &output.cut_edges);
        write_footer(
            &mut out,
            output.cut_edges.len(),
            output.total_cut_edges_count,
            input.offset.unwrap_or(0),
        );
        return out;
    }

    // Nothing to cut. When every sink is an entry point the warning above has
    // already explained why — "already unreachable" would contradict it.
    if all_sinks_uncuttable(input, &output.uncuttable_sinks) {
        let trimmed = out.trim_end().len();
        out.truncate(trimmed);
        return out;
    }
    out.push_str(NO_CUT_MESSAGE);
    out
}

fn all_sinks_uncuttable(input: &MinCutInput, uncuttable_sinks: &[String]) -> bool {
    input.sinks.iter().all(|s| uncuttable_sinks.contains(s))
}

fn write_summary(out: &mut String, input: &MinCutInput) {
    out.push_str("Min cut\n\n");
    let _ = writeln!(out, "Sinks: {}", input.sinks.join(", "));
    if !input.protected_edges.is_empty() {
        let protected: Vec<String> = input
            .protected_edges
            .iter()
            .map(|e| format_edge(&e.from, &e.to))
            .collect();
        let _ = writeln!(out, "Protected: {}", protected.join(", "));
    }
    out.push('\n');
}

fn write_uncuttable_warning(out: &mut String, uncuttable_sinks: &[String]) {
    if uncuttable_sinks.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "warning: entry points cannot be cut off by removing edges, delete them directly: {}\n",
        uncuttable_sinks.join(", ")
    );
}

fn write_table(out: &mut String, cut_edges: &[MinCutNamedEdge]) {
    let widths = column_widths(cut_edges);

    let start = out.len();
    write_cell(out, FROM_HEADER, widths[0], true);
    out.push_str(" | ");
    write_cell(out, TO_HEADER, widths[1], true);
    trim_trailing_spaces(out, start);
    out.push('\n');
    write_separator(out, '=', &widths);

    for edge in cut_edges {
        let start = out.len();
        write_cell(out, &edge.from, widths[0], true);
        out.push_str(" | ");
        write_cell(out, &edge.to, widths[1], true);
        trim_trailing_spaces(out, start);
        out.push('\n');
    }
}

fn column_widths(cut_edges: &[MinCutNamedEdge]) -> Vec<usize> {
    let from = cut_edges
        .iter()
        .map(|e| e.from.len())
        .fold(FROM_HEADER.len(), usize::max);
    let to = cut_edges
        .iter()
        .map(|e| e.to.len())
        .fold(TO_HEADER.len(), usize::max);
    vec![from, to]
}

fn write_footer(out: &mut String, shown: usize, total: usize, offset: usize) {
    let _ = write!(out, "\n{total} edge(s) to cut");
    if total > shown {
        let _ = write!(out, " (showing {shown}, offset {offset})");
    }
}

fn format_edge(from: &str, to: &str) -> String {
    format!("{from} -> {to}")
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

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
use unigraph_core::NodeIDX;
use unigraph_core::NodeSelection;
use unigraph_core::SelectOptions;
use unigraph_rpc::RpcExec;

use crate::Unigraph;
use crate::graph_cache::PerfStats;
use crate::graph_handle::GraphHandle;
use crate::graph_handle::resolve_graph_handle;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FindAncestorsInput {
    /// Graph handle — timeline ID, graph key, or GQC key.
    pub handle: GraphHandle,
    /// The node to find ancestors of.
    pub node_name: String,

    /// Which ancestors to keep — the same predicate `SearchNodes` and the
    /// `Matching` explore target take. An empty selection keeps every ancestor.
    #[serde(default)]
    pub selection: NodeSelection,
    /// When true, only return ancestors with no parents (graph entrypoints).
    ///
    /// Not a [`NodeSelection`] condition: that describes edges a node *has*,
    /// and this is a condition on the absence of every incoming edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parentless: Option<bool>,

    /// Skip first N matching results (for pagination). Defaults to 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// Maximum number of results to return. Defaults to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// When true, include a human-readable ASCII summary in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ascii: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FindAncestorsOutput {
    /// Matching ancestor node names (paginated).
    pub ancestors: Vec<String>,
    /// Total number of matching ancestors (before offset/limit).
    pub total_count: usize,
    /// Human-readable summary. Only populated when `include_ascii` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<String>,
    /// Where this request spent its time. `None` from a server predating the
    /// field — absent rather than zeroed, so it is never mistaken for a
    /// measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perf_stats: Option<PerfStats>,
}

// ── Handler ──────────────────────────────────────────────────

const DEFAULT_TTL_SECS: u64 = 5 * 60;
const DEFAULT_LIMIT: usize = 100;

impl RpcExec<Unigraph> for FindAncestorsInput {
    type Output = FindAncestorsOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<FindAncestorsOutput> {
        validate_has_predicates(&self)?;
        let ttl = Duration::from_secs(DEFAULT_TTL_SECS);
        let cached = resolve_graph_handle(&self.handle, ctx, task, ttl).await?;
        let input = self;
        task.spawn("find_ancestors", |task| async move {
            tokio::task::spawn_blocking(move || {
                find_ancestors(cached.graph, cached.perf_stats, &input, &task)
            })
            .await
            .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Validation ──────────────────────────────────────────────

fn validate_has_predicates(input: &FindAncestorsInput) -> Result<()> {
    if input.selection.is_empty() && !input.parentless.unwrap_or(false) {
        bail!("at least one predicate is required (a node selection, or parentless)");
    }
    Ok(())
}

// ── Core logic (runs in spawn_blocking) ─────────────────────

fn find_ancestors(
    ag: Arc<ArrayGraph>,
    perf_stats: PerfStats,
    input: &FindAncestorsInput,
    task: &ll::Task,
) -> Result<FindAncestorsOutput> {
    let start_idx = resolve_start_node(&ag, &input.node_name)?;
    let all_matches = collect_matching_ancestors(&ag, start_idx, input, task)?;
    let total_count = all_matches.len();
    let page = paginate(
        &all_matches,
        input.offset.unwrap_or(0),
        input.limit.unwrap_or(DEFAULT_LIMIT),
    );
    let ascii = if input.include_ascii.unwrap_or(false) {
        Some(format_ascii(&input.node_name, input, &page, total_count))
    } else {
        None
    };
    Ok(FindAncestorsOutput {
        ancestors: page,
        total_count,
        ascii,
        perf_stats: Some(perf_stats),
    })
}

fn resolve_start_node(ag: &ArrayGraph, node_name: &str) -> Result<NodeIDX> {
    ag.data
        .node_names_ordered
        .name_to_idx_log(node_name)
        .with_context(|| format!("node '{}' not found in graph", node_name))
}

fn collect_matching_ancestors(
    ag: &ArrayGraph,
    start_idx: NodeIDX,
    input: &FindAncestorsInput,
    task: &ll::Task,
) -> Result<Vec<String>> {
    let selected = selected_nodes(ag, &input.selection, task)?;
    let check_parentless = input.parentless.unwrap_or(false);
    let reverse = ag.edges_reverse();

    let mut matches: Vec<String> = reverse
        .dfs_unconfigured(&[start_idx])
        .filter(|&idx| idx != start_idx)
        .filter(|&idx| selected.as_ref().is_none_or(|set| set.contains(&idx)))
        .filter(|&idx| !check_parentless || reverse.edges(idx).next().is_none())
        .map(|idx| ag.idx_to_name(idx).to_string())
        .collect();

    matches.sort();
    Ok(matches)
}

/// The nodes the selection admits, or `None` when it admits all of them.
///
/// Evaluated once over the whole graph rather than re-derived at each ancestor:
/// the selection's conditions are index-backed, so a single pass is cheaper
/// than binding them per hop. `None` skips even that pass, which is what keeps
/// a `parentless`-only query from materializing every node in the graph.
fn selected_nodes(
    ag: &ArrayGraph,
    selection: &NodeSelection,
    task: &ll::Task,
) -> Result<Option<BTreeSet<NodeIDX>>> {
    if selection.is_empty() {
        return Ok(None);
    }
    let opts = SelectOptions {
        limit: None,
        reachable_only: false,
    };
    let selected = ag.select_nodes(selection, &opts, task)?;
    Ok(Some(selected.into_iter().collect()))
}

fn paginate(all: &[String], offset: usize, limit: usize) -> Vec<String> {
    all.iter().skip(offset).take(limit).cloned().collect()
}

// ── ASCII formatting ────────────────────────────────────────

fn format_ascii(
    node_name: &str,
    input: &FindAncestorsInput,
    page: &[String],
    total_count: usize,
) -> String {
    let mut out = String::new();
    let _ = write!(out, "Found {} ancestors of \"{}\"", total_count, node_name);
    let _ = write!(out, " matching {}", format_predicates(input));
    let _ = writeln!(out, ":");
    let _ = writeln!(out);

    let offset = input.offset.unwrap_or(0);
    for (i, name) in page.iter().enumerate() {
        let _ = writeln!(out, "  {}. {}", offset + i + 1, name);
    }

    if page.len() < total_count {
        let _ = write!(
            out,
            "\n(showing {} of {} results, offset {})",
            page.len(),
            total_count,
            offset
        );
    }
    out
}

fn format_predicates(input: &FindAncestorsInput) -> String {
    let selection = &input.selection;
    let mut parts = Vec::new();

    if let Some(name) = selection.name_condition() {
        parts.push(format!("name {:?} {:?}", name.mode, name.pattern));
    }
    for (name, condition) in &selection.properties {
        match &condition.value {
            Some(value) => parts.push(format!("{}={}", name, value)),
            None => parts.push(name.clone()),
        }
    }
    for (name, value) in &selection.metrics {
        parts.push(format!("{}={}", name, value));
    }
    for tag in &selection.incoming_tags {
        parts.push(format!("incoming-tag={}", tag));
    }
    for tag in &selection.outgoing_tags {
        parts.push(format!("outgoing-tag={}", tag));
    }
    for key in &selection.incoming_dynamic_type_keys {
        parts.push(format!("incoming-dynamic={}", key));
    }
    for key in &selection.outgoing_dynamic_type_keys {
        parts.push(format!("outgoing-dynamic={}", key));
    }
    if input.parentless.unwrap_or(false) {
        parts.push("parentless".to_string());
    }

    format!("{{{}}}", parts.join(", "))
}

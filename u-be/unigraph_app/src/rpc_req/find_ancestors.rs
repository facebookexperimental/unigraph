// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
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
use unigraph_core::PropertyIndices;
use unigraph_rpc::RpcExec;

use crate::Unigraph;
use crate::graph_handle::GraphHandle;
use crate::graph_handle::resolve_graph_handle;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FindAncestorsInput {
    /// Graph handle — timeline ID, graph key, or GQC key.
    pub handle: GraphHandle,
    /// The node to find ancestors of.
    pub node_name: String,

    /// Property predicates — all must match (AND). e.g. `{"type": "budget"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, String>>,
    /// When true, only return ancestors with no parents (graph entrypoints).
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
}

// ── Handler ──────────────────────────────────────────────────

const DEFAULT_TTL_SECS: u64 = 5 * 60;
const DEFAULT_LIMIT: usize = 100;

impl RpcExec<Unigraph> for FindAncestorsInput {
    type Output = FindAncestorsOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<FindAncestorsOutput> {
        validate_has_predicates(&self)?;
        let ttl = Duration::from_secs(DEFAULT_TTL_SECS);
        let ag = resolve_graph_handle(&self.handle, ctx, task, ttl).await?;
        let input = self;
        task.spawn("find_ancestors", |task| async move {
            tokio::task::spawn_blocking(move || find_ancestors(ag, &input, &task))
                .await
                .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Validation ──────────────────────────────────────────────

fn validate_has_predicates(input: &FindAncestorsInput) -> Result<()> {
    let has_properties = input.properties.as_ref().is_some_and(|p| !p.is_empty());
    let has_parentless = input.parentless.unwrap_or(false);
    if !has_properties && !has_parentless {
        bail!("at least one predicate is required (properties or parentless)");
    }
    Ok(())
}

// ── Core logic (runs in spawn_blocking) ─────────────────────

fn find_ancestors(
    ag: Arc<ArrayGraph>,
    input: &FindAncestorsInput,
    _task: &ll::Task,
) -> Result<FindAncestorsOutput> {
    let start_idx = resolve_start_node(&ag, &input.node_name)?;
    let all_matches = collect_matching_ancestors(&ag, start_idx, input)?;
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
) -> Result<Vec<String>> {
    // A requested property that doesn't exist in the graph can't ever match.
    let Some(property_indices) = bind_properties(ag, input) else {
        return Ok(Vec::new());
    };
    let check_parentless = input.parentless.unwrap_or(false);

    let reverse = ag.edges_reverse();

    let mut matches = Vec::new();
    for node_idx in reverse.dfs_unconfigured(&[start_idx]) {
        if node_idx == start_idx {
            continue;
        }
        if !matches_predicates(ag, node_idx, &property_indices, check_parentless) {
            continue;
        }
        matches.push(ag.idx_to_name(node_idx).to_string());
    }

    matches.sort();
    Ok(matches)
}

/// Bind the requested properties to the graph's inverted indices.
///
/// `None` means a requested property name doesn't exist in the graph at all,
/// so no node can match every predicate.
fn bind_properties<'a>(
    ag: &'a ArrayGraph,
    input: &'a FindAncestorsInput,
) -> Option<PropertyIndices<'a>> {
    let conditions = input
        .properties
        .iter()
        .flatten()
        .map(|(name, value)| (name.as_str(), Some(value.as_str())));
    PropertyIndices::bind(ag, conditions)
}

fn matches_predicates(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    property_indices: &PropertyIndices<'_>,
    check_parentless: bool,
) -> bool {
    if check_parentless && ag.edges_reverse().edges(node_idx).next().is_some() {
        return false;
    }
    property_indices.matches(node_idx)
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
    let mut parts = Vec::new();
    if let Some(props) = &input.properties {
        for (k, v) in props {
            parts.push(format!("{}={}", k, v));
        }
    }
    if input.parentless.unwrap_or(false) {
        parts.push("parentless".to_string());
    }
    format!("{{{}}}", parts.join(", "))
}

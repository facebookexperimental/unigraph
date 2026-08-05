// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::GraphNode;
use unigraph_core::NodeIDX;
use unigraph_core::PropertyIndices;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::TimelineID;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypeGen)]
pub enum SearchMode {
    #[default]
    Fuzzy,
    ExactMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SearchNodesInput {
    pub timeline_id: TimelineID,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SearchMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_properties: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SearchNodeMatch {
    pub name: String,
    pub node: GraphNode,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SearchNodesOutput {
    pub matches: Vec<SearchNodeMatch>,
}

// ── Handler ──────────────────────────────────────────────────

const DEFAULT_TTL_HOURS: u64 = 6;

impl RpcExec<Unigraph> for SearchNodesInput {
    type Output = SearchNodesOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<SearchNodesOutput> {
        let ttl = Duration::from_hours(DEFAULT_TTL_HOURS);
        let ag = ctx
            .graph_cache
            .get_latest_by_timeline(&self.timeline_id, task, ttl)
            .await?;
        let limit = self.limit.unwrap_or(30);
        let pattern = self.pattern;
        let mode = self.mode.unwrap_or_default();
        let match_properties = self.match_properties;
        let matches = task
            .spawn("search_nodes", |task| async move {
                tokio::task::spawn_blocking(move || {
                    search_nodes(
                        ag,
                        pattern.as_deref(),
                        limit,
                        &mode,
                        match_properties.as_ref(),
                        &task,
                    )
                })
                .await
                .context("spawn_blocking panicked")?
            })
            .await?;
        Ok(SearchNodesOutput { matches })
    }
}

// ── Search logic ─────────────────────────────────────────────

fn search_nodes(
    ag: Arc<ArrayGraph>,
    pattern: Option<&str>,
    limit: usize,
    mode: &SearchMode,
    match_properties: Option<&BTreeMap<String, String>>,
    task: &ll::Task,
) -> Result<Vec<SearchNodeMatch>> {
    let has_properties = match_properties.is_some_and(|p| !p.is_empty());

    let Some(property_indices) = bind_properties(&ag, match_properties) else {
        return Ok(Vec::new());
    };

    match pattern {
        Some(pat) => search_with_pattern(&ag, pat, limit, mode, &property_indices, task),
        None if has_properties => search_by_properties_only(&ag, limit, &property_indices),
        None => all_nodes(&ag, limit),
    }
}

fn to_match(ag: &ArrayGraph, name: &str, idx: NodeIDX) -> SearchNodeMatch {
    SearchNodeMatch {
        name: name.to_string(),
        node: ag.get_map_node(idx),
    }
}

fn search_with_pattern(
    ag: &ArrayGraph,
    pattern: &str,
    limit: usize,
    mode: &SearchMode,
    property_indices: &PropertyIndices<'_>,
    task: &ll::Task,
) -> Result<Vec<SearchNodeMatch>> {
    let candidates = match mode {
        SearchMode::Fuzzy => ag.search_name_fuzzy(pattern, limit, task)?,
        SearchMode::ExactMatch => match ag.data.node_names_ordered.name_to_idx_log(pattern) {
            Some(idx) => vec![(ag.idx_to_name(idx), idx)],
            None => Vec::new(),
        },
    };

    if property_indices.is_empty() {
        return Ok(candidates
            .into_iter()
            .map(|(name, idx)| to_match(ag, name, idx))
            .collect());
    }

    Ok(candidates
        .into_iter()
        .filter(|(_, idx)| property_indices.matches(*idx))
        .take(limit)
        .map(|(name, idx)| to_match(ag, name, idx))
        .collect())
}

fn search_by_properties_only(
    ag: &ArrayGraph,
    limit: usize,
    property_indices: &PropertyIndices<'_>,
) -> Result<Vec<SearchNodeMatch>> {
    Ok(property_indices
        .intersect()
        .into_iter()
        .take(limit)
        .map(|idx| to_match(ag, ag.idx_to_name(idx), idx))
        .collect())
}

fn all_nodes(ag: &ArrayGraph, limit: usize) -> Result<Vec<SearchNodeMatch>> {
    Ok((0..ag.data.node_names_ordered.len())
        .take(limit)
        .map(|i| {
            let idx = NodeIDX::from(i);
            to_match(ag, ag.idx_to_name(idx), idx)
        })
        .collect())
}

// ── Property helpers ─────────────────────────────────────────

/// Bind the requested properties to the graph's inverted indices.
///
/// `None` means a requested property name doesn't exist in the graph at all,
/// so no node can match every condition.
fn bind_properties<'a>(
    ag: &'a ArrayGraph,
    match_properties: Option<&'a BTreeMap<String, String>>,
) -> Option<PropertyIndices<'a>> {
    let conditions = match_properties
        .into_iter()
        .flatten()
        .map(|(name, value)| (name.as_str(), Some(value.as_str())));
    PropertyIndices::bind(ag, conditions)
}

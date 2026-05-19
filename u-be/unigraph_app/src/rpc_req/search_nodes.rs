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
    let property_indices = build_property_indices(&ag, match_properties);
    let has_properties = match_properties.is_some_and(|p| !p.is_empty());

    if has_properties && property_indices.len() < match_properties.unwrap().len() {
        return Ok(Vec::new());
    }

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
    property_indices: &[(&str, &BTreeMap<NodeIDX, String>)],
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
        .filter(|(_, idx)| matches_properties(*idx, property_indices))
        .take(limit)
        .map(|(name, idx)| to_match(ag, name, idx))
        .collect())
}

fn search_by_properties_only(
    ag: &ArrayGraph,
    limit: usize,
    property_indices: &[(&str, &BTreeMap<NodeIDX, String>)],
) -> Result<Vec<SearchNodeMatch>> {
    let candidates = intersect_property_indices(property_indices);
    Ok(candidates
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

fn build_property_indices<'a>(
    ag: &'a ArrayGraph,
    match_properties: Option<&'a BTreeMap<String, String>>,
) -> Vec<(&'a str, &'a BTreeMap<NodeIDX, String>)> {
    let Some(properties) = match_properties else {
        return Vec::new();
    };
    properties
        .iter()
        .filter_map(|(name, value)| {
            ag.data
                .node_metadata
                .properties
                .get(name)
                .map(|index| (value.as_str(), index))
        })
        .collect()
}

fn matches_properties(
    node_idx: NodeIDX,
    property_indices: &[(&str, &BTreeMap<NodeIDX, String>)],
) -> bool {
    property_indices.iter().all(|&(expected_value, index)| {
        index
            .get(&node_idx)
            .is_some_and(|v| v.as_str() == expected_value)
    })
}

fn intersect_property_indices(
    property_indices: &[(&str, &BTreeMap<NodeIDX, String>)],
) -> Vec<NodeIDX> {
    if property_indices.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<_> = property_indices.iter().enumerate().collect();
    sorted.sort_by_key(|(_, (_, index))| index.len());

    let (first_pos, (first_value, first_index)) = sorted[0];
    let mut candidates: Vec<NodeIDX> = first_index
        .iter()
        .filter(|(_, v)| v.as_str() == *first_value)
        .map(|(idx, _)| *idx)
        .collect();

    for &(i, (expected_value, index)) in &sorted[1..] {
        if i == first_pos {
            continue;
        }
        candidates.retain(|idx| {
            index
                .get(idx)
                .is_some_and(|v| v.as_str() == *expected_value)
        });
    }

    candidates.sort();
    candidates
}

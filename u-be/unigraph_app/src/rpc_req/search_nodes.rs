// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::GraphNode;
use unigraph_core::NodeSelection;
use unigraph_core::SelectOptions;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::TimelineID;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

/// Find nodes matching a [`NodeSelection`] — by name, properties, or edge tags.
///
/// The name mode defaults to `Substring`. Typeahead callers that want the
/// subsequence, shortest-first behaviour have to ask for `Fuzzy` explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SearchNodesInput {
    pub timeline_id: TimelineID,

    /// Which nodes to match. An empty selection matches every node.
    #[serde(default)]
    pub selection: NodeSelection,

    /// Maximum number of matches to return. Defaults to 30.
    ///
    /// Under the `Fuzzy` name mode this is the top-K cap, so the result is the
    /// best `limit` matches rather than a page of a larger set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
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
const DEFAULT_LIMIT: usize = 30;

impl RpcExec<Unigraph> for SearchNodesInput {
    type Output = SearchNodesOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<SearchNodesOutput> {
        let ttl = Duration::from_hours(DEFAULT_TTL_HOURS);
        let ag = ctx
            .graph_cache
            .get_latest_by_timeline(&self.timeline_id, task, ttl)
            .await?;
        let input = self;
        task.spawn("search_nodes", |task| async move {
            tokio::task::spawn_blocking(move || search_nodes(ag, &input, &task))
                .await
                .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Search logic (runs in spawn_blocking) ────────────────────

/// Unlike the tree table, a name search reaches nodes the traversal config
/// pruned — you can search for something to find out it was excluded.
fn search_nodes(
    ag: Arc<ArrayGraph>,
    input: &SearchNodesInput,
    task: &ll::Task,
) -> Result<SearchNodesOutput> {
    let opts = SelectOptions {
        limit: Some(input.limit.unwrap_or(DEFAULT_LIMIT)),
        reachable_only: false,
    };
    let matched = ag.select_nodes(&input.selection, &opts, task)?;

    Ok(SearchNodesOutput {
        matches: matched
            .into_iter()
            .map(|idx| SearchNodeMatch {
                name: ag.idx_to_name(idx).to_string(),
                node: ag.get_map_node(idx),
            })
            .collect(),
    })
}

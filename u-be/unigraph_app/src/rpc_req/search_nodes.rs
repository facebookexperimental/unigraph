// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::TimelineID;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SearchNodesInput {
    pub timeline_id: TimelineID,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SearchNodesOutput {
    pub matches: Vec<String>,
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
        let matches = task
            .spawn("search_nodes", |task| async move {
                tokio::task::spawn_blocking(move || search_nodes(ag, &pattern, limit, &task))
                    .await
                    .context("spawn_blocking panicked")?
            })
            .await?;
        Ok(SearchNodesOutput { matches })
    }
}

fn search_nodes(
    ag: Arc<ArrayGraph>,
    pattern: &str,
    limit: usize,
    task: &ll::Task,
) -> Result<Vec<String>> {
    Ok(ag
        .search_name_fuzzy(pattern, limit, task)?
        .into_iter()
        .map(|(name, _idx)| name.to_string())
        .collect())
}

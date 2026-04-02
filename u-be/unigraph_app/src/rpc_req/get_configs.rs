// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::TraversalConfig;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_rpc::RpcExec;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetConfigsInput {
    pub traversal_configs: Vec<TraversalConfigKey>,
    pub graph_query_configs: Vec<GraphQueryConfigKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetConfigsOutput {
    pub traversal_configs: Vec<TraversalConfig>,
    pub graph_query_configs: Vec<GraphQueryConfig>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for GetConfigsInput {
    type Output = GetConfigsOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<GetConfigsOutput> {
        let traversal_configs = fetch_traversal_configs(ctx, self.traversal_configs, task).await?;
        let graph_query_configs =
            fetch_graph_query_configs(ctx, self.graph_query_configs, task).await?;

        Ok(GetConfigsOutput {
            traversal_configs,
            graph_query_configs,
        })
    }
}

async fn fetch_traversal_configs(
    ctx: &Unigraph,
    keys: Vec<TraversalConfigKey>,
    task: &ll::Task,
) -> Result<Vec<TraversalConfig>> {
    let mut configs = Vec::with_capacity(keys.len());
    for key in &keys {
        configs.push(ctx.db.configs.fetch_traversal_config(key, task).await?);
    }
    Ok(configs)
}

async fn fetch_graph_query_configs(
    ctx: &Unigraph,
    keys: Vec<GraphQueryConfigKey>,
    task: &ll::Task,
) -> Result<Vec<GraphQueryConfig>> {
    let mut configs = Vec::with_capacity(keys.len());
    for key in &keys {
        configs.push(ctx.db.configs.fetch_graph_query_config(key, task).await?);
    }
    Ok(configs)
}

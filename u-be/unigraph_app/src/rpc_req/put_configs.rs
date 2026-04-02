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
pub struct PutConfigsInput {
    pub traversal_configs: Vec<TraversalConfig>,
    pub graph_query_configs: Vec<GraphQueryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct PutConfigsOutput {
    pub traversal_configs: Vec<TraversalConfigKey>,
    pub graph_query_configs: Vec<GraphQueryConfigKey>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for PutConfigsInput {
    type Output = PutConfigsOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<PutConfigsOutput> {
        let traversal_configs = store_traversal_configs(ctx, self.traversal_configs, task).await?;
        let graph_query_configs =
            store_graph_query_configs(ctx, self.graph_query_configs, task).await?;

        Ok(PutConfigsOutput {
            traversal_configs,
            graph_query_configs,
        })
    }
}

async fn store_traversal_configs(
    ctx: &Unigraph,
    configs: Vec<TraversalConfig>,
    task: &ll::Task,
) -> Result<Vec<TraversalConfigKey>> {
    let mut keys = Vec::with_capacity(configs.len());
    for config in &configs {
        keys.push(ctx.db.configs.store_traversal_config(config, task).await?);
    }
    Ok(keys)
}

async fn store_graph_query_configs(
    ctx: &Unigraph,
    configs: Vec<GraphQueryConfig>,
    task: &ll::Task,
) -> Result<Vec<GraphQueryConfigKey>> {
    let mut keys = Vec::with_capacity(configs.len());
    for config in &configs {
        keys.push(
            ctx.db
                .configs
                .store_graph_query_config(config, task)
                .await?,
        );
    }
    Ok(keys)
}

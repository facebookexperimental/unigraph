// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_rpc::RpcExec;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryInput {
    /// The query config: which graph, optional roots, optional traversal.
    pub query: GraphQueryConfig,
}

#[derive(Debug, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryOutput {
    pub package: ArrayGraphSerializablePackageBase64,
    pub graph_query_config: GraphQueryConfig,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for GraphQueryInput {
    type Output = GraphQueryOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<GraphQueryOutput> {
        let ttl = Duration::from_mins(5);
        let ag = ctx.graph_cache.get_explored(&self.query, task, ttl).await?;

        let resolved_gqc = GraphQueryConfig {
            handle: self.query.handle,
            roots: self.query.roots,
            traversal: ag
                .runtime
                .state
                .traversal_config
                .as_ref()
                .map(|tc| unigraph_core::config_query::TraversalOverride::Inline(tc.clone())),
        };

        // Cheap refcount clone of the shared data — no deep copy. `pack` reads
        // through the `Arc`, so the blocking task just needs a `'static` owner.
        let ag_data = ag.data.clone();
        let package = task
            .spawn("pack", |task| async move {
                tokio::task::spawn_blocking(move || {
                    let config = ArrayGraphSerializablePackageConfig::default();
                    ag_data.pack(&config, &task)
                })
                .await
                .context("spawn_blocking panicked")?
            })
            .await?;

        Ok(GraphQueryOutput {
            package: package.into_base_64(),
            graph_query_config: resolved_gqc,
        })
    }
}

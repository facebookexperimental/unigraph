// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::MapGraph;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_rpc::RpcExec;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryMapGraphInput {
    /// The query config: which graph, optional roots, optional traversal.
    pub query: GraphQueryConfig,
}

/// Like [`GraphQueryOutput`](super::GraphQueryOutput), but returns the graph as a
/// plain [`MapGraph`] instead of a packed `ArrayGraph`.
///
/// Meant for smaller queries and tiny graphs that don't need the `ArrayGraph`
/// CSR packing/compression — the caller gets a human-readable, directly
/// serializable graph back.
#[derive(Debug, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryMapGraphOutput {
    pub map_graph: MapGraph,
    pub graph_query_config: GraphQueryConfig,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for GraphQueryMapGraphInput {
    type Output = GraphQueryMapGraphOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<GraphQueryMapGraphOutput> {
        let ttl = Duration::from_mins(5);
        let ag = ctx.graph_cache.get_explored(&self.query, task, ttl).await?;

        let resolved_gqc = super::graph_query::resolve_query_config(self.query, &ag);

        // `to_map_graph` reads through the shared `Arc` (cheap to move — no deep
        // copy of the payload), but the walk itself is CPU-heavy, so run it on a
        // blocking thread to keep the runtime free.
        let map_graph = task
            .spawn("to_map_graph", |_task| async move {
                tokio::task::spawn_blocking(move || ag.to_map_graph())
                    .await
                    .context("spawn_blocking panicked")?
            })
            .await?;

        Ok(GraphQueryMapGraphOutput {
            map_graph,
            graph_query_config: resolved_gqc,
        })
    }
}

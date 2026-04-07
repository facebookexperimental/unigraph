// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::types::NodeName;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::GraphKeyOrTimelineID;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryInput {
    /// Inline graph query config. Either this or `graph_query_config_key` must be set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_query_config: Option<GraphQueryConfig>,
    /// Key referencing a stored graph query config. Resolved server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_query_config_key: Option<GraphQueryConfigKey>,
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
        let mut gqc = resolve_gqc(ctx, &self, task).await?;
        let handle = gqc
            .handle
            .as_deref()
            .context("graph_query_config.handle is required")?;
        let mut ag = fetch_graph(ctx, handle, task).await?;
        ag = extract_subgraph(ag, &gqc.roots, task).await?;

        if gqc.traversal_config.is_none() {
            gqc.traversal_config = ag.traversal_config.clone();
        }
        ag.traversal_config = gqc.traversal_config.clone();

        let package = tokio::task::spawn_blocking(move || {
            let config = ArrayGraphSerializablePackageConfig::default();
            ag.pack(&config)
        })
        .await??;

        Ok(GraphQueryOutput {
            package: package.into_base_64(),
            graph_query_config: gqc,
        })
    }
}

async fn resolve_gqc(
    ctx: &Unigraph,
    input: &GraphQueryInput,
    task: &ll::Task,
) -> Result<GraphQueryConfig> {
    match (&input.graph_query_config, &input.graph_query_config_key) {
        (Some(gqc), _) => Ok(gqc.clone()),
        (_, Some(key)) => ctx.db.configs.fetch_graph_query_config(key, task).await,
        (None, None) => bail!("either graph_query_config or graph_query_config_key must be set"),
    }
}

async fn fetch_graph(
    ctx: &Unigraph,
    handle: &str,
    task: &ll::Task,
) -> Result<ArrayGraphSerializable> {
    let parsed: GraphKeyOrTimelineID = handle.parse()?;
    let (_key, graph) = match parsed {
        GraphKeyOrTimelineID::GraphKey(key) => {
            let graph = ctx.db.graph.fetch(&key, task).await?;
            (key, graph)
        }
        GraphKeyOrTimelineID::TimelineID(tid) => ctx.db.graph.fetch_latest(&tid, task).await?,
    };
    Ok(graph)
}

async fn extract_subgraph(
    ag: ArrayGraphSerializable,
    roots: &std::collections::BTreeSet<NodeName>,
    task: &ll::Task,
) -> Result<ArrayGraphSerializable> {
    if roots.is_empty() {
        return Ok(ag);
    }

    let root_idxs: Vec<_> = roots
        .iter()
        .filter_map(|name| ag.node_names_ordered.name_to_idx_log(name.as_str()))
        .collect();

    tokio::task::spawn_blocking(move || {
        ag.into_array_graph()
            .get_reachable_subgraph_unconfigured(&root_idxs)
    })
    .await
    .context("spawn_blocking panicked")?
}

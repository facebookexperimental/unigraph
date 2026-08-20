// Copyright (c) Meta Platforms, Inc. and affiliates.

// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Graph handle resolution — bridges the core `GraphHandle` type with the
//! app-level cache and storage.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use unigraph_core::ArrayGraph;
pub use unigraph_core::GraphHandle;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_storage_core::GraphKey;

use crate::Unigraph;

/// Resolve a `GraphHandle` to an `ArrayGraph`, using the cache where possible.
pub async fn resolve_graph_handle(
    handle: &GraphHandle,
    ctx: &Unigraph,
    task: &ll::Task,
    ttl: Duration,
) -> Result<Arc<ArrayGraph>> {
    let (_graph_key, graph) = resolve_graph_handle_with_key(handle, ctx, task, ttl).await?;
    Ok(graph)
}

/// Like [`resolve_graph_handle`], but also returns the [`GraphKey`] of the
/// concrete snapshot the handle landed on.
///
/// Two of the three handle forms name a graph only indirectly — a bare
/// `TimelineID` means "whatever is latest", and a GQC key means "whatever its
/// embedded reference resolves to". Both move as frames are ingested, so a
/// caller that reports back to a human has to be able to say which snapshot it
/// actually read.
pub async fn resolve_graph_handle_with_key(
    handle: &GraphHandle,
    ctx: &Unigraph,
    task: &ll::Task,
    ttl: Duration,
) -> Result<(GraphKey, Arc<ArrayGraph>)> {
    match handle {
        GraphHandle::GqcKey(_) => {
            let gqc = GraphQueryConfig {
                handle: handle.clone(),
                roots: None,
                traversal: None,
            };
            ctx.graph_cache.get_explored_with_key(&gqc, task, ttl).await
        }
        GraphHandle::TimelineID(tid) => {
            ctx.graph_cache
                .get_latest_by_timeline_with_key(tid, task, ttl)
                .await
        }
        GraphHandle::GraphKey(key) => {
            let ag_ser = ctx.db.graph.fetch(key, task).await?;
            Ok((key.clone(), Arc::new(ag_ser.into_array_graph(task)?)))
        }
    }
}

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

use crate::Unigraph;

/// Resolve a `GraphHandle` to an `ArrayGraph`, using the cache where possible.
pub async fn resolve_graph_handle(
    handle: &GraphHandle,
    ctx: &Unigraph,
    task: &ll::Task,
    ttl: Duration,
) -> Result<Arc<ArrayGraph>> {
    match handle {
        GraphHandle::GqcKey(_) => {
            let gqc = GraphQueryConfig {
                handle: handle.clone(),
                roots: None,
                traversal: None,
            };
            ctx.graph_cache.get_explored(&gqc, task, ttl).await
        }
        GraphHandle::TimelineID(tid) => {
            ctx.graph_cache.get_latest_by_timeline(tid, task, ttl).await
        }
        GraphHandle::GraphKey(key) => {
            let ag_ser = ctx.db.graph.fetch(key, task).await?;
            Ok(Arc::new(ag_ser.into_array_graph(task)?))
        }
    }
}

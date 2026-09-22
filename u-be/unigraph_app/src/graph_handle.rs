// Copyright (c) Meta Platforms, Inc. and affiliates.

// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Graph handle resolution — bridges the core `GraphHandle` type with the
//! app-level cache and storage.

use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
pub use unigraph_core::GraphHandle;
use unigraph_core::config_query::GraphQueryConfig;

use crate::Unigraph;
use crate::graph_cache::CachedGraph;
use crate::graph_cache::PerfStats;

/// Resolve a `GraphHandle` to an `ArrayGraph`, using the cache where possible.
///
/// A `{timeline}~{id}` handle bypasses the cache entirely — it is fetched from
/// storage on every call, and so always reports a miss.
pub async fn resolve_graph_handle(
    handle: &GraphHandle,
    ctx: &Unigraph,
    task: &ll::Task,
    ttl: Duration,
) -> Result<CachedGraph> {
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
            let started = Instant::now();
            let ag_ser = ctx.db.graph.fetch(key, task).await?;
            let graph = Arc::new(ag_ser.into_array_graph(task)?);
            Ok(CachedGraph {
                graph,
                graph_key: key.clone(),
                perf_stats: PerfStats {
                    cache_hit: false,
                    cache_elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                },
            })
        }
    }
}

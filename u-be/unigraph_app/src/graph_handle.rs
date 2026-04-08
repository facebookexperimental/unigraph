// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Unified graph handle — resolves timeline IDs, graph keys, and GQC keys
//! to a cached `ArrayGraph`.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use unigraph_core::ArrayGraph;
use unigraph_core::config_key::ConfigKeyLike;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphKeyOrTimelineID;
use unigraph_storage_core::TimelineID;

use crate::Unigraph;

/// A parsed graph handle — resolves to a cached or fetched `ArrayGraph`.
///
/// Handles come in three forms:
/// - `gqc_{hash}` — GQC key (content-addressed config with embedded graph ref)
/// - `{timeline}~{id}` — GraphKey (specific snapshot)
/// - `{timeline}` — TimelineID (latest graph)
#[derive(Debug, Clone)]
pub enum GraphHandle {
    GqcKey(GraphQueryConfigKey),
    GraphKey(GraphKey),
    TimelineID(TimelineID),
}

impl GraphHandle {
    /// Resolve this handle to an `ArrayGraph`, using the cache where possible.
    pub async fn resolve(
        &self,
        ctx: &Unigraph,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<Arc<ArrayGraph>> {
        match self {
            GraphHandle::GqcKey(key) => ctx.graph_cache.get_by_gqc_key(key, task, ttl).await,
            GraphHandle::TimelineID(tid) => {
                ctx.graph_cache.get_latest_by_timeline(tid, task, ttl).await
            }
            GraphHandle::GraphKey(key) => {
                let ag_ser = ctx.db.graph.fetch(key, task).await?;
                Ok(Arc::new(ag_ser.into_array_graph()))
            }
        }
    }
}

impl FromStr for GraphHandle {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.starts_with(GraphQueryConfigKey::PREFIX) {
            return Ok(GraphHandle::GqcKey(s.parse()?));
        }

        match s.parse::<GraphKeyOrTimelineID>()? {
            GraphKeyOrTimelineID::GraphKey(key) => Ok(GraphHandle::GraphKey(key)),
            GraphKeyOrTimelineID::TimelineID(tid) => Ok(GraphHandle::TimelineID(tid)),
        }
    }
}

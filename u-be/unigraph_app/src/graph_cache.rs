// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! In-memory graph cache with LRU eviction, TTL, and stampede prevention.
//!
//! `GraphCache` caches fully-prepared `ArrayGraph` instances (fetched, roots-filtered,
//! traversal-applied) keyed by `GraphQueryConfigKey`. Multiple concurrent requests for
//! the same key share a single computation — the first caller fetches, others wait.
//!
//! ```text
//! get_by_gqc_key("gqc-abc", task)
//!   ├─ cache hit   → Arc::clone(cached_graph)
//!   └─ cache miss  → resolve config → fetch graph → apply roots → apply traversal
//!                    → store Arc<ArrayGraph> → return clone
//! ```

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use lru::LruCache;
use tokio::sync::Mutex;
use unigraph_core::ArrayGraph;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::GraphKeyOrTimelineID;

type ComputationSlot = Arc<Mutex<Option<Arc<ArrayGraph>>>>;

/// In-memory LRU cache for prepared graphs, keyed by `GraphQueryConfigKey`.
///
/// Designed for interactive exploration where the same graph is queried repeatedly
/// with different view parameters (target node, sort, pagination). The graph identity
/// is fixed by the config key — only view parameters change between requests.
///
/// ## Stampede prevention
///
/// When multiple requests arrive for the same uncached key, only the first caller
/// fetches from storage. Others wait on the same computation slot and receive
/// `Arc::clone` of the result.
///
/// ## Thread safety
///
/// `GraphCache` is `Clone` (all fields are `Arc`-wrapped). The outer `Mutex` is held
/// only briefly to check/insert LRU slots. The per-key inner `Mutex` serializes
/// computation for that key without blocking other keys.
#[derive(Clone)]
pub struct GraphCache {
    db: UnigraphDb,
    by_gqc_key: Arc<Mutex<LruCache<GraphQueryConfigKey, ComputationSlot>>>,
    ttl: Duration,
}

impl GraphCache {
    pub fn new(db: UnigraphDb, capacity: usize, ttl: Duration) -> Self {
        let cap = std::num::NonZero::new(capacity).expect("cache capacity must be > 0");
        Self {
            db,
            by_gqc_key: Arc::new(Mutex::new(LruCache::new(cap))),
            ttl,
        }
    }

    /// Retrieve a prepared `ArrayGraph` by config key, fetching from storage on miss.
    ///
    /// The returned graph has roots filtering and traversal config already applied.
    /// It is shared via `Arc` — all callers get the same immutable instance.
    pub async fn get_by_gqc_key(
        &self,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<Arc<ArrayGraph>> {
        let slot = self.get_or_create_gqc_slot(key).await;
        self.resolve_gqc_slot(slot, key, task).await
    }
}

// ── Implementation ───────────────────────────────────────────────

impl GraphCache {
    /// Check the `by_gqc_key` LRU for an existing slot, or create an empty one.
    async fn get_or_create_gqc_slot(&self, key: &GraphQueryConfigKey) -> ComputationSlot {
        let mut lru = self.by_gqc_key.lock().await;
        if let Some(slot) = lru.get(key) {
            Arc::clone(slot)
        } else {
            let slot: ComputationSlot = Arc::new(Mutex::new(None));
            lru.put(key.clone(), Arc::clone(&slot));
            slot
        }
    }

    /// Lock the slot: if populated, return the cached value; otherwise compute and cache it.
    async fn resolve_gqc_slot(
        &self,
        slot: ComputationSlot,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<Arc<ArrayGraph>> {
        let mut guard = slot.lock().await;

        if let Some(cached) = guard.as_ref() {
            return Ok(Arc::clone(cached));
        }

        let graph = Arc::new(self.fetch_and_prepare(key, task).await?);
        *guard = Some(Arc::clone(&graph));

        self.schedule_eviction(key.clone());

        Ok(graph)
    }

    /// Fetch graph from storage and apply roots + traversal config.
    async fn fetch_and_prepare(
        &self,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<ArrayGraph> {
        let gqc = self.db.configs.fetch_graph_query_config(key, task).await?;
        let ag_ser = fetch_graph(&self.db, &gqc, task).await?;
        let mut ag = ag_ser.into_array_graph();
        apply_traversal(&mut ag, &gqc)?;
        Ok(ag)
    }

    /// Schedule TTL-based eviction for a key.
    fn schedule_eviction(&self, key: GraphQueryConfigKey) {
        if self.ttl.is_zero() {
            return;
        }
        let cache = Arc::clone(&self.by_gqc_key);
        let ttl = self.ttl;
        tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            if let Ok(mut lru) = cache.try_lock() {
                lru.pop(&key);
            }
        });
    }
}

// ── Graph fetching (moved from explore_graph.rs) ─────────────────

async fn fetch_graph(
    db: &UnigraphDb,
    gqc: &GraphQueryConfig,
    task: &ll::Task,
) -> Result<unigraph_core::ArrayGraphSerializable> {
    let handle = gqc
        .handle
        .as_deref()
        .context("graph_query_config.handle is required")?;
    let parsed: GraphKeyOrTimelineID = handle.parse()?;
    let (_key, mut ag) = match parsed {
        GraphKeyOrTimelineID::GraphKey(key) => {
            let graph = db.graph.fetch(&key, task).await?;
            (key, graph)
        }
        GraphKeyOrTimelineID::TimelineID(tid) => db.graph.fetch_latest(&tid, task).await?,
    };

    if !gqc.roots.is_empty() {
        let root_idxs: Vec<_> = gqc
            .roots
            .iter()
            .filter_map(|name| ag.node_names_ordered.name_to_idx_log(name.as_str()))
            .collect();
        ag = ag
            .into_array_graph()
            .get_reachable_subgraph_unconfigured(&root_idxs)?;
    }

    Ok(ag)
}

fn apply_traversal(ag: &mut ArrayGraph, gqc: &GraphQueryConfig) -> Result<()> {
    let tvc = gqc
        .traversal_config
        .as_ref()
        .or(ag.state.traversal_config.as_ref());
    if let Some(tvc) = tvc {
        ag.apply_traversal_config(tvc.clone())?;
    }
    Ok(())
}

// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! In-memory graph cache with LRU eviction, per-entry TTL, and stampede prevention.
//!
//! `GraphCache` caches `Arc<ArrayGraph>` instances across two LRU caches:
//!
//! - **`by_gqc_key`** — keyed by `GraphQueryConfigKey`. Stores fully-prepared graphs
//!   (fetched, roots-filtered, traversal-applied). Used by ExploreGraph RPC.
//!
//! - **`by_timeline_latest`** — keyed by `TimelineID`. Stores the latest raw graph
//!   for a timeline (no roots/traversal). Used by SearchNodes RPC.
//!
//! Each entry carries its own TTL, set by the caller at fetch time.
//!
//! ```text
//! get_by_gqc_key("gqc_abc", task, 5min)
//!   ├─ cache hit   → Arc::clone(cached_graph)
//!   └─ cache miss  → resolve config → fetch graph → apply roots → apply traversal
//!                    → store entry with TTL → return Arc::clone
//!
//! get_latest_by_timeline("my-timeline", task, 60s)
//!   ├─ cache hit   → Arc::clone(cached_graph)
//!   └─ cache miss  → fetch_latest → into_array_graph → store → return Arc::clone
//! ```

use std::fmt::Debug;
use std::hash::Hash;
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
use unigraph_storage_core::TimelineID;

/// A cached graph — the value stored in each LRU slot.
struct ArrayGraphCacheEntry {
    graph: Arc<ArrayGraph>,
}

type CacheSlot = Arc<Mutex<Option<ArrayGraphCacheEntry>>>;

/// In-memory LRU cache for prepared graphs.
///
/// Two independent LRUs serve different access patterns:
///
/// - `by_gqc_key`: for interactive exploration — graph identity fixed by config key,
///   only view parameters (target, sort, pagination) change between requests.
/// - `by_timeline_latest`: for node search — caches the latest raw graph for a timeline,
///   avoids re-fetching on every keystroke.
///
/// ## Stampede prevention
///
/// When multiple requests arrive for the same uncached key, only the first caller
/// fetches from storage. Others wait on the same `CacheSlot` and receive
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
    by_gqc_key: Arc<Mutex<LruCache<GraphQueryConfigKey, CacheSlot>>>,
    by_timeline_latest: Arc<Mutex<LruCache<TimelineID, CacheSlot>>>,
}

impl GraphCache {
    pub fn new(db: UnigraphDb, capacity: usize) -> Self {
        let cap = std::num::NonZero::new(capacity).expect("cache capacity must be > 0");
        Self {
            db,
            by_gqc_key: Arc::new(Mutex::new(LruCache::new(cap))),
            by_timeline_latest: Arc::new(Mutex::new(LruCache::new(cap))),
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
        ttl: Duration,
    ) -> Result<Arc<ArrayGraph>> {
        let slot = get_or_create_slot(&self.by_gqc_key, key).await;
        self.resolve_gqc_slot(slot, key, task, ttl).await
    }

    /// Retrieve the latest raw `ArrayGraph` for a timeline, fetching from storage on miss.
    ///
    /// No roots filtering or traversal config is applied — this is the full graph
    /// as stored. Useful for node search and other timeline-level operations.
    pub async fn get_latest_by_timeline(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<Arc<ArrayGraph>> {
        let slot = get_or_create_slot(&self.by_timeline_latest, timeline_id).await;
        self.resolve_timeline_slot(slot, timeline_id, task, ttl)
            .await
    }
}

// ── Slot resolution ─────────────────────────────────────────────

impl GraphCache {
    /// Lock the GQC slot: if populated, return cached; otherwise fetch, prepare, and cache.
    async fn resolve_gqc_slot(
        &self,
        slot: CacheSlot,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<Arc<ArrayGraph>> {
        let mut guard = slot.lock().await;

        if let Some(entry) = guard.as_ref() {
            return Ok(Arc::clone(&entry.graph));
        }

        let graph = Arc::new(self.fetch_and_prepare_gqc(key, task).await?);
        *guard = Some(ArrayGraphCacheEntry {
            graph: Arc::clone(&graph),
        });

        schedule_eviction(&self.by_gqc_key, key.clone(), ttl);

        Ok(graph)
    }

    /// Lock the timeline slot: if populated, return cached; otherwise fetch and cache.
    async fn resolve_timeline_slot(
        &self,
        slot: CacheSlot,
        timeline_id: &TimelineID,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<Arc<ArrayGraph>> {
        let mut guard = slot.lock().await;

        if let Some(entry) = guard.as_ref() {
            return Ok(Arc::clone(&entry.graph));
        }

        let graph = Arc::new(self.fetch_latest_graph(timeline_id, task).await?);
        *guard = Some(ArrayGraphCacheEntry {
            graph: Arc::clone(&graph),
        });

        schedule_eviction(&self.by_timeline_latest, timeline_id.clone(), ttl);

        Ok(graph)
    }
}

// ── Fetch helpers ───────────────────────────────────────────────

impl GraphCache {
    /// Fetch graph by GQC key and apply roots + traversal config.
    async fn fetch_and_prepare_gqc(
        &self,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<ArrayGraph> {
        let gqc = self.db.configs.fetch_graph_query_config(key, task).await?;
        let ag_ser = fetch_graph(&self.db, &gqc, task).await?;
        let mut ag = ag_ser.into_array_graph(task)?;
        apply_traversal(&mut ag, &gqc)?;
        Ok(ag)
    }

    /// Fetch the latest graph for a timeline — raw, no roots/traversal applied.
    async fn fetch_latest_graph(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<ArrayGraph> {
        let (_key, ag_ser) = self.db.graph.fetch_latest(timeline_id, task).await?;
        task.spawn("into_array_graph", |task| async move {
            tokio::task::spawn_blocking(move || ag_ser.into_array_graph(&task))
                .await
                .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Generic LRU helpers ─────────────────────────────────────────

/// Check the LRU for an existing slot, or create an empty one.
async fn get_or_create_slot<K: Clone + Eq + Hash>(
    lru: &Arc<Mutex<LruCache<K, CacheSlot>>>,
    key: &K,
) -> CacheSlot {
    let mut guard = lru.lock().await;
    if let Some(slot) = guard.get(key) {
        Arc::clone(slot)
    } else {
        let slot: CacheSlot = Arc::new(Mutex::new(None));
        guard.put(key.clone(), Arc::clone(&slot));
        slot
    }
}

/// Schedule TTL-based eviction for a key from an LRU cache.
fn schedule_eviction<K: Clone + Eq + Hash + Debug + Send + 'static>(
    lru: &Arc<Mutex<LruCache<K, CacheSlot>>>,
    key: K,
    ttl: Duration,
) {
    if ttl.is_zero() {
        return;
    }
    let cache = Arc::clone(lru);
    tokio::spawn(async move {
        tokio::time::sleep(ttl).await;
        if let Ok(mut guard) = cache.try_lock() {
            guard.pop(&key);
        }
    });
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
            .into_array_graph(task)?
            .get_reachable_subgraph_unconfigured(&root_idxs)?;
    }

    Ok(ag)
}

fn apply_traversal(ag: &mut ArrayGraph, gqc: &GraphQueryConfig) -> Result<()> {
    let tvc = gqc
        .traversal_config
        .as_ref()
        .or(ag.runtime.state.traversal_config.as_ref());
    if let Some(tvc) = tvc {
        ag.apply_traversal_config(tvc.clone())?;
    }
    Ok(())
}

// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! In-memory graph cache with LRU eviction, per-entry TTL, and stampede prevention.
//!
//! `GraphCache` caches `Arc<ArrayGraph>` instances across two LRU caches:
//!
//! - **`explore`** — keyed by `ExploreCacheKey` (derived from `GraphQueryConfig`).
//!   Stores fully-prepared graphs (fetched, roots-filtered, traversal-applied).
//!   Used by GraphQuery and ExploreGraph RPCs.
//!
//! - **`by_timeline_latest`** — keyed by `TimelineID`. Stores the latest raw graph
//!   for a timeline (no roots/traversal). Used by SearchNodes RPC.
//!
//! Each entry carries its own TTL, set by the caller at fetch time.
//!
//! ```text
//! get_explored(GraphQueryConfig { handle, roots?, traversal? }, task, 5min)
//!   ├─ cache hit   → Arc::clone(cached_graph)
//!   └─ cache miss  → resolve handle → fetch graph → apply roots → apply traversal
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
use unigraph_core::ExploreCacheKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_db::UnigraphDb;
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
/// - `explore`: for interactive exploration — graph identity fixed by a
///   `GraphQueryConfig` (handle + optional roots/traversal overrides).
/// - `by_timeline_latest`: for node search — caches the latest raw graph for a
///   timeline, avoids re-fetching on every keystroke.
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
    explore: Arc<Mutex<LruCache<ExploreCacheKey, CacheSlot>>>,
    by_timeline_latest: Arc<Mutex<LruCache<TimelineID, CacheSlot>>>,
}

impl GraphCache {
    pub fn new(db: UnigraphDb, capacity: usize) -> Self {
        let cap = std::num::NonZero::new(capacity).expect("cache capacity must be > 0");
        Self {
            db,
            explore: Arc::new(Mutex::new(LruCache::new(cap))),
            by_timeline_latest: Arc::new(Mutex::new(LruCache::new(cap))),
        }
    }

    /// Retrieve a prepared `ArrayGraph` by query config, fetching from storage on miss.
    ///
    /// Resolution order:
    /// 1. Resolve handle → fetch the base graph (recursing into GQC if needed).
    /// 2. Roots: `gqc.roots` > inner GQC roots > no filtering.
    /// 3. Traversal: `gqc.traversal` > inner GQC traversal > `graph.traversal_config`.
    ///
    /// The returned graph has roots filtering and traversal config already applied.
    /// It is shared via `Arc` — all callers get the same immutable instance.
    pub async fn get_explored(
        &self,
        gqc: &GraphQueryConfig,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<Arc<ArrayGraph>> {
        let cache_key = gqc.cache_key();
        let this = self.clone();
        let gqc = gqc.clone();
        task.spawn("get_explored", |task| async move {
            task.data("cache_key", cache_key.to_string());
            let slot = get_or_create_slot(&this.explore, &cache_key).await;
            let mut guard = slot.lock().await;

            if let Some(entry) = guard.as_ref() {
                task.data("cache", "hit");
                return Ok(Arc::clone(&entry.graph));
            }

            task.data("cache", "miss");
            let (_, graph) = this.db.resolve_graph_query_config(&gqc, &task).await?;
            let graph = Arc::new(graph);
            *guard = Some(ArrayGraphCacheEntry {
                graph: Arc::clone(&graph),
            });

            schedule_eviction(&this.explore, cache_key, ttl);

            Ok(graph)
        })
        .await
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

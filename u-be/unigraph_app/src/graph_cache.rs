// Copyright (c) Meta Platforms, Inc. and affiliates.

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
//! - **`twins`** — keyed by a pair of `ExploreCacheKey`s. Stores merged
//!   `TwinGraph`s for left-vs-right comparison. Used by the ExploreDelta RPC.
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
//!
//! get_twin(left_gqc, right_gqc, task, 5min)
//!   ├─ cache hit   → Arc::clone(cached_twin)
//!   └─ cache miss  → resolve both sides (concurrently, untraversed)
//!                    → super-root both → traverse each → merge → Arc::clone
//! ```

use std::fmt;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use lru::LruCache;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::ExploreCacheKey;
use unigraph_core::TraversalConfig;
use unigraph_core::TwinGraph;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_db::UnigraphDb;
use unigraph_db::apply_traversal;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::TimelineID;

/// Where a request's time went. Every field is prefixed by the stage it
/// measures, so new stages can be added alongside these without renaming them.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TypeGen)]
pub struct PerfStats {
    /// True when the graph came out of the LRU without touching storage.
    pub cache_hit: bool,

    /// Wall time the caller spent inside the cache lookup.
    ///
    /// This includes time spent blocked behind another caller's in-flight fetch
    /// for the same key, so a hit is not automatically fast — a slow hit means
    /// the entry was being computed by someone else while this caller waited.
    pub cache_elapsed_ms: f64,
}

/// A graph obtained through [`GraphCache`], with the key it resolved to and the
/// stats for the lookup that produced it.
pub struct CachedGraph {
    pub graph: Arc<ArrayGraph>,

    /// The concrete graph snapshot the query/handle resolved to. Bare timelines
    /// and GQC keys both move as frames are ingested, so callers that report
    /// back to a human need this to say *what they actually read*.
    pub graph_key: GraphKey,

    pub perf_stats: PerfStats,
}

/// A cached graph — the value stored in each LRU slot.
struct ArrayGraphCacheEntry {
    graph: Arc<ArrayGraph>,
    /// The resolved key identifying the concrete graph snapshot `graph` was
    /// reconstructed from. Stored so it is available on cache **hits** too, not
    /// just the miss that computed the graph.
    graph_key: GraphKey,
}

/// One LRU slot. Empty until the first caller fills it; the inner `Mutex` is
/// what serializes concurrent misses for the same key.
type Slot<V> = Arc<Mutex<Option<V>>>;
type CacheSlot = Slot<ArrayGraphCacheEntry>;
type TwinSlot = Slot<Arc<TwinGraph>>;

/// Identifies a merged `TwinGraph` by the two query configs it was built from:
/// `(left, right)`. Order matters — swapping the sides flips every delta's sign.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TwinCacheKey(ExploreCacheKey, ExploreCacheKey);

impl fmt::Display for TwinCacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.0, self.1)
    }
}

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
    twins: Arc<Mutex<LruCache<TwinCacheKey, TwinSlot>>>,
}

impl GraphCache {
    pub fn new(db: UnigraphDb, capacity: usize) -> Self {
        let cap = std::num::NonZero::new(capacity).expect("cache capacity must be > 0");
        Self {
            db,
            explore: Arc::new(Mutex::new(LruCache::new(cap))),
            by_timeline_latest: Arc::new(Mutex::new(LruCache::new(cap))),
            twins: Arc::new(Mutex::new(LruCache::new(cap))),
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
    ///
    /// The resolved [`GraphKey`] and the [`PerfStats`] for the lookup come back
    /// alongside it; the key is available on hits too, since it is stored on the
    /// cache entry.
    pub async fn get_explored(
        &self,
        gqc: &GraphQueryConfig,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<CachedGraph> {
        let cache_key = gqc.cache_key();
        let this = self.clone();
        let gqc = gqc.clone();
        task.spawn("get_explored", |task| async move {
            let started = Instant::now();
            task.data("cache_key", cache_key.to_string());
            let slot = get_or_create_slot(&this.explore, &cache_key).await;
            let mut guard = slot.lock().await;

            if let Some(entry) = guard.as_ref() {
                return Ok(entry.to_cached(record(&task, true, started)));
            }

            let (graph_key, graph) = this
                .db
                .resolve_graph_query_config(&gqc, true, &task)
                .await?;
            let entry = ArrayGraphCacheEntry {
                graph: Arc::new(graph),
                graph_key,
            };
            let cached = entry.to_cached(record(&task, false, started));
            *guard = Some(entry);

            schedule_eviction(&this.explore, cache_key, ttl);

            Ok(cached)
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
    ) -> Result<CachedGraph> {
        let started = Instant::now();
        let slot = get_or_create_slot(&self.by_timeline_latest, timeline_id).await;
        self.resolve_timeline_slot(slot, timeline_id, task, ttl, started)
            .await
    }

    /// Retrieve the merged [`TwinGraph`] for a pair of query configs, building
    /// it on miss.
    ///
    /// Both sides are resolved **independently of the `explore` LRU**: a
    /// `TwinGraph` owns its two `ArrayGraph`s, and super-rooting mutates them
    /// through `Arc::make_mut` — handing it a shared `Arc<ArrayGraph>` would
    /// deep-clone a tens-of-megabytes payload. The cost is one extra fetch when
    /// a handle is used both standalone and as a twin side.
    ///
    /// Caching the twin (rather than its two sides) is what makes repeated
    /// requests cheap: the changed-nodes graph is built lazily behind a
    /// `OnceLock` *inside* `TwinGraph`, so every "changed nodes only" request
    /// after the first is free.
    pub async fn get_twin(
        &self,
        left: &GraphQueryConfig,
        right: &GraphQueryConfig,
        task: &ll::Task,
        ttl: Duration,
    ) -> Result<(PerfStats, Arc<TwinGraph>)> {
        let cache_key = TwinCacheKey(left.cache_key(), right.cache_key());
        let this = self.clone();
        let (left, right) = (left.clone(), right.clone());

        task.spawn("get_twin", |task| async move {
            let started = Instant::now();
            task.data("cache_key", cache_key.to_string());
            let slot = get_or_create_slot(&this.twins, &cache_key).await;
            let mut guard = slot.lock().await;

            if let Some(twin) = guard.as_ref() {
                return Ok((record(&task, true, started), Arc::clone(twin)));
            }

            let twin = Arc::new(this.build_twin(&left, &right, &task).await?);
            *guard = Some(Arc::clone(&twin));

            schedule_eviction(&this.twins, cache_key, ttl);

            Ok((record(&task, false, started), twin))
        })
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
        started: Instant,
    ) -> Result<CachedGraph> {
        let mut guard = slot.lock().await;

        if let Some(entry) = guard.as_ref() {
            return Ok(entry.to_cached(record(task, true, started)));
        }

        let (graph_key, graph) = self.fetch_latest_graph(timeline_id, task).await?;
        let entry = ArrayGraphCacheEntry {
            graph: Arc::new(graph),
            graph_key,
        };
        let cached = entry.to_cached(record(task, false, started));
        *guard = Some(entry);

        schedule_eviction(&self.by_timeline_latest, timeline_id.clone(), ttl);

        Ok(cached)
    }
}

// ── Fetch helpers ───────────────────────────────────────────────

impl GraphCache {
    /// Fetch the latest graph for a timeline — raw, no roots/traversal applied.
    ///
    /// Also returns the [`GraphKey`] of the snapshot that was fetched.
    async fn fetch_latest_graph(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<(GraphKey, ArrayGraph)> {
        let (key, ag_ser) = self.db.graph.fetch_latest(timeline_id, task).await?;
        let graph = task
            .spawn("into_array_graph", |task| async move {
                tokio::task::spawn_blocking(move || ag_ser.into_array_graph(&task))
                    .await
                    .context("spawn_blocking panicked")?
            })
            .await?;
        Ok((key, graph))
    }
}

impl GraphCache {
    /// Resolve both sides, then super-root → traverse → merge.
    ///
    /// This can't just call `resolve_graph_query_config` twice: the super root
    /// has to be decided across *both* sides together, and it must land before
    /// traversal so it gets tiered and made reachable by the same pass as every
    /// other node.
    async fn build_twin(
        &self,
        left: &GraphQueryConfig,
        right: &GraphQueryConfig,
        task: &ll::Task,
    ) -> Result<TwinGraph> {
        let ((_, l_graph, l_traversal), (_, r_graph, r_traversal)) = tokio::try_join!(
            self.db.resolve_gqc_untraversed(left, task),
            self.db.resolve_gqc_untraversed(right, task),
        )?;

        task.spawn("merge_twin", |_task| async move {
            tokio::task::spawn_blocking(move || {
                merge_twin(l_graph, l_traversal, r_graph, r_traversal)
            })
            .await
            .context("spawn_blocking panicked")?
        })
        .await
    }
}

fn merge_twin(
    l: ArrayGraph,
    l_traversal: Option<TraversalConfig>,
    r: ArrayGraph,
    r_traversal: Option<TraversalConfig>,
) -> Result<TwinGraph> {
    let (mut l, mut r) = TwinGraph::add_super_roots(l, r)?;
    apply_traversal(&mut l, l_traversal.as_ref())?;
    apply_traversal(&mut r, r_traversal.as_ref())?;
    TwinGraph::from_prepared(l, r)
}

// ── Stats ───────────────────────────────────────────────────────

impl ArrayGraphCacheEntry {
    fn to_cached(&self, perf_stats: PerfStats) -> CachedGraph {
        CachedGraph {
            graph: Arc::clone(&self.graph),
            graph_key: self.graph_key.clone(),
            perf_stats,
        }
    }
}

/// Build the [`PerfStats`] for a finished lookup and mirror them into the task log.
fn record(task: &ll::Task, cache_hit: bool, started: Instant) -> PerfStats {
    let cache_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    task.data("cache", if cache_hit { "hit" } else { "miss" });
    task.data("cache_elapsed_ms", format!("{cache_elapsed_ms:.1}"));
    PerfStats {
        cache_hit,
        cache_elapsed_ms,
    }
}

// ── Generic LRU helpers ─────────────────────────────────────────

/// Check the LRU for an existing slot, or create an empty one.
async fn get_or_create_slot<K: Clone + Eq + Hash, V>(
    lru: &Arc<Mutex<LruCache<K, Slot<V>>>>,
    key: &K,
) -> Slot<V> {
    let mut guard = lru.lock().await;
    if let Some(slot) = guard.get(key) {
        Arc::clone(slot)
    } else {
        let slot: Slot<V> = Arc::new(Mutex::new(None));
        guard.put(key.clone(), Arc::clone(&slot));
        slot
    }
}

/// Schedule TTL-based eviction for a key from an LRU cache.
fn schedule_eviction<K: Clone + Eq + Hash + Debug + Send + 'static, V: Send + 'static>(
    lru: &Arc<Mutex<LruCache<K, Slot<V>>>>,
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! In-memory cache for content-addressed configs.
//!
//! `ConfigCache` sits in front of [`Configs`](unigraph_db::Configs) and caches
//! `Arc<TraversalConfig>` / `Arc<GraphQueryConfig>` in two LRUs.
//!
//! ```text
//! get_traversal_config(tvc_1a2b3c4d)
//!   ├─ cache hit   → Arc::clone(cached_config)
//!   └─ cache miss  → SQL row → blob (Manifold) → zstd decode → serde_json
//!                    → store → return Arc::clone
//! ```
//!
//! # Why there is no TTL
//!
//! Unlike [`GraphCache`](crate::GraphCache), entries here never expire. Config
//! keys are *content-addressed* — `TraversalConfigKey::from_blob` hashes the
//! serialized config, so a given key maps to one byte-identical config forever.
//! A stale read is impossible by construction, which is also why there is no
//! invalidation path: storing a changed config produces a different key.
//!
//! # Why this is worth caching
//!
//! A miss is expensive and completely uninstrumented below `get_config_row`:
//! a pool checkout, a Manifold blob read, then a zstd decode and a
//! `serde_json` parse of a `BTreeMap<String, BTreeMap<String, Decision>>` — one
//! `String` allocation and B-tree insert per edge. Measured at ~2.9s for a WWW
//! project TVC, against 65ms for the SQL row itself. Any caller that resolves
//! the same config more than once (graph ingestion loops, `GraphCache` misses
//! over a warm timeline) pays that repeatedly for a byte-identical answer.

use std::hash::Hash;
use std::sync::Arc;

use anyhow::Result;
use lru::LruCache;
use tokio::sync::Mutex;
use unigraph_core::TraversalConfig;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_db::UnigraphDb;

/// Entries per config LRU. Configs are small next to graphs (a WWW project TVC
/// is a few MB parsed), and a deployment only has a handful of live keys, so
/// this is sized to hold them all rather than to bound memory tightly.
pub const DEFAULT_CONFIG_CACHE_CAPACITY: usize = 256;

/// A slot shared by everyone racing for the same key. Empty until the first
/// caller fills it.
type CacheSlot<T> = Arc<Mutex<Option<Arc<T>>>>;

type ConfigLru<K, V> = Arc<Mutex<LruCache<K, CacheSlot<V>>>>;

/// LRU cache for content-addressed configs, shared across threads.
///
/// `ConfigCache` is `Clone` (every field is `Arc`-wrapped). The outer `Mutex`
/// is held only long enough to look up or insert a slot; the per-key inner
/// `Mutex` serializes the fetch for that key alone.
///
/// ## Stampede prevention
///
/// Concurrent callers for the same uncached key contend on that key's slot, so
/// exactly one performs the fetch and the rest receive `Arc::clone` of its
/// result. This matters most for the case the cache exists to fix: N workers
/// starting at once and all wanting the same TVC.
#[derive(Clone)]
pub struct ConfigCache {
    db: UnigraphDb,
    traversal: ConfigLru<TraversalConfigKey, TraversalConfig>,
    graph_query: ConfigLru<GraphQueryConfigKey, GraphQueryConfig>,
}

impl ConfigCache {
    pub fn new(db: UnigraphDb, capacity: usize) -> Self {
        let cap = std::num::NonZero::new(capacity).expect("cache capacity must be > 0");
        Self {
            db,
            traversal: Arc::new(Mutex::new(LruCache::new(cap))),
            graph_query: Arc::new(Mutex::new(LruCache::new(cap))),
        }
    }

    /// Fetch a traversal config by key, hitting storage only on a cache miss.
    #[ll::task(tags(l3))]
    pub async fn get_traversal_config(
        &self,
        key: &TraversalConfigKey,
        task: &ll::Task,
    ) -> Result<Arc<TraversalConfig>> {
        let slot = get_or_create_slot(&self.traversal, key).await;
        let mut guard = slot.lock().await;

        if let Some(cached) = guard.as_ref() {
            task.data("cache", "hit");
            return Ok(Arc::clone(cached));
        }

        task.data("cache", "miss");
        let config = Arc::new(self.db.configs.fetch_traversal_config(key, &task).await?);
        *guard = Some(Arc::clone(&config));
        Ok(config)
    }

    /// Fetch a graph query config by key, hitting storage only on a cache miss.
    ///
    /// Caches the GQC alone. Its traversal comes back as an unresolved
    /// `TraversalOverride::Key`, and resolving that is left to the caller —
    /// route it through [`get_traversal_config`](Self::get_traversal_config)
    /// so the expensive half is cached too.
    #[ll::task(tags(l3))]
    pub async fn get_graph_query_config(
        &self,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<Arc<GraphQueryConfig>> {
        let slot = get_or_create_slot(&self.graph_query, key).await;
        let mut guard = slot.lock().await;

        if let Some(cached) = guard.as_ref() {
            task.data("cache", "hit");
            return Ok(Arc::clone(cached));
        }

        task.data("cache", "miss");
        let config = Arc::new(self.db.configs.fetch_graph_query_config(key, &task).await?);
        *guard = Some(Arc::clone(&config));
        Ok(config)
    }
}

// ── LRU helpers ─────────────────────────────────────────────────

/// Look up an existing slot for `key`, or install an empty one.
async fn get_or_create_slot<K: Clone + Eq + Hash, V>(
    lru: &ConfigLru<K, V>,
    key: &K,
) -> CacheSlot<V> {
    let mut guard = lru.lock().await;
    if let Some(slot) = guard.get(key) {
        Arc::clone(slot)
    } else {
        let slot: CacheSlot<V> = Arc::new(Mutex::new(None));
        guard.put(key.clone(), Arc::clone(&slot));
        slot
    }
}

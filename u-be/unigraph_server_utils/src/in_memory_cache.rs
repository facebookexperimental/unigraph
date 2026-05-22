// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use lru::LruCache;
use tokio::sync::Mutex;

type AsyncMakeEntry<K, V> =
    Arc<dyn Fn(K) -> Pin<Box<dyn Future<Output = Result<V>> + Send>> + Send + Sync>;

type ComputationGuard<V> = Arc<Mutex<Option<Arc<V>>>>;

/// An in-memory cache for expensive async operations with TTL and LRU eviction.
///
/// This cache is designed for storing graphs fetched from storage systems where:
/// - Loading graphs from storage is expensive (disk I/O, network calls, deserialization)
/// - Graphs can be large and consume significant RAM if cached indefinitely
/// - Recent graphs are more likely to be accessed again (temporal locality)
///
/// ## Features
///
/// - **Async Support**: Caches results of async functions
/// - **Cache Stampede Prevention**: Multiple concurrent requests for the same key only compute once
/// - **TTL (Time-To-Live)**: Automatic expiration prevents unbounded memory growth
/// - **LRU Eviction**: Least recently used items are evicted when capacity is reached
/// - **Cloneable**: Multiple components can share the same cache instance
/// - **Thread-Safe**: Safe for concurrent access across multiple threads/tasks
#[derive(Clone)]
pub struct InMemoryCache<K: Eq + Hash + Clone + Send + 'static, V: Send + Sync + 'static> {
    cache: Arc<Mutex<LruCache<K, ComputationGuard<V>>>>,
    make_entry: AsyncMakeEntry<K, V>,
    ttl: std::time::Duration,
}

impl<K: Eq + Hash + Clone + Send + 'static, V: Send + Sync + 'static> InMemoryCache<K, V> {
    /// Creates a new cache with the specified capacity, entry factory function, and TTL.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of entries to cache before LRU eviction occurs
    /// * `ttl` - Time-to-live for cached entries. Use `Duration::ZERO` to disable expiration
    /// * `make_entry` - Async function that creates a new value when cache miss occurs
    pub fn new<F, Fut>(
        capacity: std::num::NonZero<usize>,
        ttl: std::time::Duration,
        make_entry: F,
    ) -> Self
    where
        F: Fn(K) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<V>> + Send + 'static,
    {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(capacity))),
            make_entry: Arc::new(move |key| Box::pin(make_entry(key))),
            ttl,
        }
    }

    /// Gets a value from the cache, computing it if necessary.
    ///
    /// If the key exists in cache, returns the cached `Arc<V>` immediately.
    /// If the key doesn't exist, calls the `make_entry` function to compute the value,
    /// caches it, and returns the result. Multiple concurrent requests for the same
    /// key will only trigger one computation.
    ///
    /// # Arguments
    /// * `key` - The key to look up or compute
    pub async fn get(&self, key: K) -> Result<Arc<V>> {
        // First, check if we have a slot for this key
        let computation_guard: ComputationGuard<V> = {
            let mut cache = self.cache.lock().await;
            if let Some(guard) = cache.get(&key) {
                Arc::clone(guard)
            } else {
                // Create a new computation guard for this key
                let guard: ComputationGuard<V> = Arc::new(Mutex::new(None));
                cache.put(key.clone(), Arc::clone(&guard));
                guard
            }
        };

        // Now acquire the computation lock for this specific key
        let mut slot = computation_guard.lock().await;

        // Check if the value has already been computed
        if let Some(value) = slot.as_ref() {
            return Ok(Arc::clone(value));
        }

        // We need to compute the value
        let value = Arc::new((self.make_entry)(key.clone()).await?);
        *slot = Some(Arc::clone(&value));

        // Spawn TTL cleanup task
        if !self.ttl.is_zero() {
            let cache_ref = Arc::clone(&self.cache);
            let ttl = self.ttl;
            let key_for_cleanup = key.clone();

            tokio::spawn(async move {
                tokio::time::sleep(ttl).await;

                // Try to remove the key from cache after TTL expires
                if let Ok(mut cache) = cache_ref.try_lock() {
                    cache.pop(&key_for_cleanup);
                }
                // If we can't get the lock, that's fine - the entry will be evicted later
                // or when someone tries to access it and finds it expired
            });
        }

        Ok(value)
    }

    /// Creates a new value using the entry factory function, bypassing all caching logic.
    ///
    /// This function directly calls the `make_entry` function provided during cache creation,
    /// without checking the cache, storing results, or applying TTL. Useful when you need
    /// to force a fresh computation or when caching is not desired for specific operations.
    ///
    /// # Arguments
    /// * `key` - The key to pass to the entry factory function
    pub async fn make_entry(&self, key: K) -> Result<V> {
        (self.make_entry)(key).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use k9::snapshot;
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_cache_basic_functionality() -> anyhow::Result<()> {
        // Create a cache that doubles the input number after a small delay
        let cache = InMemoryCache::new(
            std::num::NonZero::new(2).unwrap(),
            Duration::from_mins(1),
            |key: &i32| {
                let key = *key; // Copy the value to avoid lifetime issues
                async move {
                    // Simulate some async work
                    sleep(Duration::from_millis(10)).await;
                    Ok(key * 2)
                }
            },
        );

        // First call should compute the value
        let result1 = cache.get(&5).await?;
        k9::assert_equal!(*result1, 10);

        // Second call should return cached value (same Arc)
        let result2 = cache.get(&5).await?;
        k9::assert_equal!(*result2, 10);
        assert!(Arc::ptr_eq(&result1, &result2));

        // Different key should compute new value
        let result3 = cache.get(&3).await?;
        k9::assert_equal!(*result3, 6);
        assert!(!Arc::ptr_eq(&result1, &result3));

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_error_handling() -> anyhow::Result<()> {
        let cache = InMemoryCache::new(
            std::num::NonZero::new(1).unwrap(),
            Duration::from_mins(1),
            |key: &i32| {
                let key = *key; // Copy the value
                async move {
                    if key < 0 {
                        anyhow::bail!("Negative numbers not allowed");
                    }
                    Ok(key)
                }
            },
        );

        // Valid key should work
        let result = cache.get(&5).await?;
        k9::assert_equal!(*result, 5);

        // Invalid key should return error
        let error_result = cache.get(&-1).await.unwrap_err();

        snapshot!(error_result.to_string(), "Negative numbers not allowed");

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_capacity_limit() -> anyhow::Result<()> {
        let cache = InMemoryCache::new(
            std::num::NonZero::new(2).unwrap(), // Only 2 items
            Duration::from_mins(1),
            |key: String| async move { Ok(key.len()) },
        );

        // Fill the cache
        let result1 = cache.get("hello".to_string()).await?;
        let result2 = cache.get("world".to_string()).await?;
        k9::assert_equal!(*result1, 5);
        k9::assert_equal!(*result2, 5);

        // Add third item, should evict the first
        let result3 = cache.get("rust".to_string()).await?;
        k9::assert_equal!(*result3, 4);

        // First item should be evicted and recomputed
        let result1_new = cache.get("hello".to_string()).await?;
        k9::assert_equal!(*result1_new, 5);
        assert!(!Arc::ptr_eq(&result1, &result1_new)); // Should be different Arc

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_stampede_prevention() -> anyhow::Result<()> {
        use std::sync::Arc as StdArc;
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::Ordering;

        let computation_counter = StdArc::new(AtomicU32::new(0));

        let cache = InMemoryCache::new(
            std::num::NonZero::new(10).unwrap(),
            Duration::from_mins(1),
            {
                let counter = StdArc::clone(&computation_counter);
                move |key: i32| {
                    let counter = StdArc::clone(&counter);
                    async move {
                        // Increment counter to track how many times computation happens
                        counter.fetch_add(1, Ordering::SeqCst);

                        // Simulate expensive computation
                        sleep(Duration::from_millis(100)).await;

                        Ok(key * 2)
                    }
                }
            },
        );

        // Spawn multiple concurrent requests for the same key
        let key = 42;
        let cache = StdArc::new(cache);
        let mut handles = Vec::new();

        for _ in 0..5 {
            let cache_clone = StdArc::clone(&cache);
            let handle = tokio::spawn(async move { cache_clone.get(key).await });
            handles.push(handle);
        }

        // Wait for all requests to complete
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await??);
        }

        // All results should be the same value
        for result in &results {
            k9::assert_equal!(**result, 84); // 42 * 2
        }

        // All results should be the same Arc (same memory location)
        for i in 1..results.len() {
            assert!(Arc::ptr_eq(&results[0], &results[i]));
        }

        // Most importantly: computation should have happened only once!
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_ttl_functionality() -> anyhow::Result<()> {
        use std::sync::Arc as StdArc;
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::Ordering;

        let computation_counter = StdArc::new(AtomicU32::new(0));

        let cache = InMemoryCache::new(
            std::num::NonZero::new(10).unwrap(),
            Duration::from_millis(200), // Short TTL for testing
            {
                let counter = StdArc::clone(&computation_counter);
                move |key: i32| {
                    let counter = StdArc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                        Ok(key * 10)
                    }
                }
            },
        );

        let key = 42;

        // First call should compute the value
        let result1 = cache.get(key).await?;
        k9::assert_equal!(*result1, 420);
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 1);

        // Second call (before TTL expires) should return cached value
        let result2 = cache.get(key).await?;
        k9::assert_equal!(*result2, 420);
        assert!(Arc::ptr_eq(&result1, &result2)); // Same Arc
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 1); // No recomputation

        // Wait for TTL to expire
        sleep(Duration::from_millis(250)).await;

        // Third call (after TTL expires) should recompute
        let result3 = cache.get(key).await?;
        k9::assert_equal!(*result3, 420);
        assert!(!Arc::ptr_eq(&result1, &result3)); // Different Arc
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 2); // Recomputed

        Ok(())
    }

    #[tokio::test]
    async fn test_zero_ttl_disables_expiration() -> anyhow::Result<()> {
        use std::sync::Arc as StdArc;
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::Ordering;

        let computation_counter = StdArc::new(AtomicU32::new(0));

        let cache = InMemoryCache::new(
            std::num::NonZero::new(10).unwrap(),
            Duration::ZERO, // Zero TTL should disable expiration
            {
                let counter = StdArc::clone(&computation_counter);
                move |key: i32| {
                    let counter = StdArc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                        Ok(key * 100)
                    }
                }
            },
        );

        let key = 123;

        // First call should compute the value
        let result1 = cache.get(key).await?;
        k9::assert_equal!(*result1, 12300);
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 1);

        // Wait longer than any reasonable TTL
        sleep(Duration::from_millis(100)).await;

        // Second call should still return cached value (no expiration with zero TTL)
        let result2 = cache.get(key).await?;
        k9::assert_equal!(*result2, 12300);
        assert!(Arc::ptr_eq(&result1, &result2)); // Same Arc
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 1); // No recomputation

        Ok(())
    }

    #[tokio::test]
    async fn test_make_entry_bypasses_cache() -> anyhow::Result<()> {
        use std::sync::Arc as StdArc;
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::Ordering;

        let computation_counter = StdArc::new(AtomicU32::new(0));

        let cache = InMemoryCache::new(
            std::num::NonZero::new(10).unwrap(),
            Duration::from_mins(1),
            {
                let counter = StdArc::clone(&computation_counter);
                move |key: i32| {
                    let counter = StdArc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                        Ok(key * 3)
                    }
                }
            },
        );

        let key = 15;

        // First, put something in the cache
        let cached_result = cache.get(key).await?;
        k9::assert_equal!(*cached_result, 45);
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 1);

        // Now use make_entry - should bypass cache and create new value
        let direct_result = cache.make_entry(key).await?;
        k9::assert_equal!(direct_result, 45); // Same value
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 2); // But computed again!

        // Verify cache still has the original value
        let cached_result2 = cache.get(key).await?;
        k9::assert_equal!(*cached_result2, 45);
        assert!(Arc::ptr_eq(&cached_result, &cached_result2)); // Same Arc as before
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 2); // No additional computation

        // make_entry with different key should also work
        let direct_result2 = cache.make_entry(20).await?;
        k9::assert_equal!(direct_result2, 60);
        k9::assert_equal!(computation_counter.load(Ordering::SeqCst), 3);

        Ok(())
    }
}

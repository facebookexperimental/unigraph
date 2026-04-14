// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Explore cache key — a composite key for caching explored graphs.
//!
//! An `ExploreKey` identifies a specific "view" of a graph: which graph to
//! fetch (via handle), optional roots override, and optional traversal override.
//! Two identical `ExploreKey`s always produce the same `ExploreCacheKey`, which
//! is used as an LRU cache key.
//!
//! ```text
//! ExploreKey { handle, roots?, traversal? }
//!   → serialize deterministically (BTreeSet/BTreeMap guarantee order)
//!   → xxh3_64 hash
//!   → ExploreCacheKey("exp_{hash:016x}")
//! ```

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;

use crate::config_key::TraversalConfigKey;
use crate::traversal::TraversalConfig;
use crate::types::NodeName;

/// How to override the traversal config for an explored graph.
#[derive(Debug, Clone, Serialize, Deserialize, typegen::TypeGen, PartialEq)]
pub enum TraversalOverride {
    /// Full inline traversal config.
    Inline(TraversalConfig),
    /// Reference to a stored traversal config by key.
    Key(TraversalConfigKey),
}

/// Composite key for the explore cache.
///
/// - `handle` — which graph: a timeline ID (`"my_timeline"`), graph key
///   (`"my_timeline~123"`), or GQC key (`"gqc_abc123"`).
/// - `roots` — if present, overrides the handle's roots (GQC roots or
///   graph entry points).
/// - `traversal` — if present, overrides the handle's traversal config.
#[derive(Debug, Clone, Serialize, Deserialize, typegen::TypeGen, PartialEq)]
pub struct ExploreKey {
    pub handle: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub roots: Option<BTreeSet<NodeName>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub traversal: Option<TraversalOverride>,
}

/// Content-addressed cache key derived from an `ExploreKey`.
///
/// Format: `"exp_{xxh3_64_hex}"` (e.g. `"exp_1a2b3c4d5e6f7890"`).
/// Two identical `ExploreKey`s always produce the same `ExploreCacheKey`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExploreCacheKey(String);

impl ExploreKey {
    /// Compute the content-addressed cache key for this explore key.
    pub fn cache_key(&self) -> ExploreCacheKey {
        let json = serde_json::to_vec(self).expect("ExploreKey serialization cannot fail");
        let hash = xxhash_rust::xxh3::xxh3_64(&json);
        ExploreCacheKey(format!("exp_{hash:016x}"))
    }
}

impl fmt::Display for ExploreCacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_explore_keys_produce_identical_cache_keys() {
        // Insert roots in different order — BTreeSet normalizes.
        let mut roots_a = BTreeSet::new();
        roots_a.insert("z".to_string());
        roots_a.insert("a".to_string());
        roots_a.insert("m".to_string());

        let mut roots_b = BTreeSet::new();
        roots_b.insert("a".to_string());
        roots_b.insert("m".to_string());
        roots_b.insert("z".to_string());

        let key_a = ExploreKey {
            handle: "my_timeline".to_string(),
            roots: Some(roots_a),
            traversal: None,
        };
        let key_b = ExploreKey {
            handle: "my_timeline".to_string(),
            roots: Some(roots_b),
            traversal: None,
        };

        assert_eq!(key_a, key_b);
        assert_eq!(key_a.cache_key(), key_b.cache_key());
    }

    #[test]
    fn different_handles_produce_different_cache_keys() {
        let a = ExploreKey {
            handle: "timeline_a".to_string(),
            roots: None,
            traversal: None,
        };
        let b = ExploreKey {
            handle: "timeline_b".to_string(),
            roots: None,
            traversal: None,
        };

        assert_ne!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn roots_override_changes_cache_key() {
        let base = ExploreKey {
            handle: "my_timeline".to_string(),
            roots: None,
            traversal: None,
        };
        let with_roots = ExploreKey {
            handle: "my_timeline".to_string(),
            roots: Some(BTreeSet::from(["root_a".to_string()])),
            traversal: None,
        };

        assert_ne!(base.cache_key(), with_roots.cache_key());
    }

    #[test]
    fn traversal_key_changes_cache_key() {
        let base = ExploreKey {
            handle: "my_timeline".to_string(),
            roots: None,
            traversal: None,
        };
        let with_tvc = ExploreKey {
            handle: "my_timeline".to_string(),
            roots: None,
            traversal: Some(TraversalOverride::Key(TraversalConfigKey(
                "tvc_1234567890abcdef".to_string(),
            ))),
        };

        assert_ne!(base.cache_key(), with_tvc.cache_key());
    }

    #[test]
    fn deserialized_explore_keys_produce_same_cache_key() {
        let json = r#"{
            "handle": "my_timeline",
            "roots": ["c", "a", "b"]
        }"#;

        let k1: ExploreKey = serde_json::from_str(json).unwrap();
        let k2: ExploreKey = serde_json::from_str(json).unwrap();

        assert_eq!(k1.cache_key(), k2.cache_key());
    }
}

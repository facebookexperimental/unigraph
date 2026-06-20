// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Query configuration types for graph exploration.
//!
//! `GraphQueryConfig` bundles a graph handle, optional roots, and optional
//! traversal override into a single config for querying a graph.
//!
//! Also contains `TraversalOverride` (inline or stored-key reference) and
//! `ExploreCacheKey` (content-addressed LRU cache key).

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;

use crate::config_key::TraversalConfigKey;
use crate::graph_handle::GraphHandle;
use crate::traversal::TraversalConfig;
use crate::types::NodeName;

/// Configuration for querying a graph — which graph to query, where to start,
/// and how to traverse.
///
/// - `handle`: identifies the graph (timeline, snapshot, or saved GQC key)
/// - `roots`: optional entry points override. `None` = use defaults,
///   `Some(empty)` = explicitly empty roots (no entrypoints).
/// - `traversal`: optional traversal override (inline config or stored key)
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    PartialEq,
    typegen::TypeGen,
    unigraph_delta::Deltable
)]
pub struct GraphQueryConfig {
    pub handle: GraphHandle,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub roots: Option<BTreeSet<NodeName>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub traversal: Option<TraversalOverride>,
}

impl GraphQueryConfig {
    /// Compute the content-addressed cache key for this query config.
    pub fn cache_key(&self) -> ExploreCacheKey {
        let json = serde_json::to_vec(self).expect("GraphQueryConfig serialization cannot fail");
        let hash = xxhash_rust::xxh3::xxh3_64(&json);
        ExploreCacheKey::from_hash(hash)
    }
}

/// How to override the traversal config for an explored graph.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    typegen::TypeGen,
    PartialEq,
    unigraph_delta::Deltable
)]
#[expect(
    clippy::large_enum_variant,
    reason = "Inline variant is the common case"
)]
pub enum TraversalOverride {
    /// Full inline traversal config.
    Inline(TraversalConfig),
    /// Reference to a stored traversal config by key.
    Key(TraversalConfigKey),
}

/// Content-addressed cache key derived from a `GraphQueryConfig`.
///
/// Format: `"exp_{xxh3_64_hex}"` (e.g. `"exp_1a2b3c4d5e6f7890"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExploreCacheKey(String);

impl ExploreCacheKey {
    /// Create from a precomputed xxh3_64 hash.
    pub fn from_hash(hash: u64) -> Self {
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
    use std::collections::BTreeMap;

    use unigraph_delta::Deltable;

    use super::*;
    use crate::traversal::Decision;
    use crate::traversal::TraversalConfig;

    fn test_handle() -> GraphHandle {
        "my_timeline~42".parse().unwrap()
    }

    /// Constructs two identical `GraphQueryConfig`s by inserting fields in
    /// different order, then asserts they produce the same serialized key.
    #[test]
    fn identical_configs_produce_identical_serialized_keys() {
        let mut roots_a = BTreeSet::new();
        roots_a.insert("alpha".to_string());
        roots_a.insert("beta".to_string());
        roots_a.insert("gamma".to_string());

        let mut force_nodes_a = BTreeMap::new();
        force_nodes_a.insert("x".to_string(), Decision::include());
        force_nodes_a.insert("y".to_string(), Decision::exclude());
        force_nodes_a.insert("z".to_string(), Decision::include());

        let config_a = GraphQueryConfig {
            handle: test_handle(),
            roots: Some(roots_a),
            traversal: Some(TraversalOverride::Inline(TraversalConfig {
                force_nodes: Some(force_nodes_a),
                ..Default::default()
            })),
        };

        let mut roots_b = BTreeSet::new();
        roots_b.insert("gamma".to_string());
        roots_b.insert("alpha".to_string());
        roots_b.insert("beta".to_string());

        let mut force_nodes_b = BTreeMap::new();
        force_nodes_b.insert("z".to_string(), Decision::include());
        force_nodes_b.insert("x".to_string(), Decision::include());
        force_nodes_b.insert("y".to_string(), Decision::exclude());

        let config_b = GraphQueryConfig {
            handle: test_handle(),
            roots: Some(roots_b),
            traversal: Some(TraversalOverride::Inline(TraversalConfig {
                force_nodes: Some(force_nodes_b),
                ..Default::default()
            })),
        };

        assert_eq!(config_a, config_b, "configs should be equal via PartialEq");

        let json_a = serde_json::to_string(&config_a).unwrap();
        let json_b = serde_json::to_string(&config_b).unwrap();
        assert_eq!(
            json_a, json_b,
            "identical configs must serialize to identical JSON for use as cache keys"
        );
    }

    /// Deserializing the same JSON twice must produce configs with identical
    /// serialized keys.
    #[test]
    fn deserialized_configs_produce_identical_serialized_keys() {
        let json = r#"{
            "handle": "my_timeline~42",
            "roots": ["gamma", "alpha", "beta"],
            "traversal": {
                "Inline": {
                    "force_nodes": {
                        "z": { "include": true },
                        "x": { "include": true },
                        "y": { "include": false }
                    }
                }
            }
        }"#;

        let config_1: GraphQueryConfig = serde_json::from_str(json).unwrap();
        let config_2: GraphQueryConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config_1, config_2);

        let key_1 = serde_json::to_string(&config_1).unwrap();
        let key_2 = serde_json::to_string(&config_2).unwrap();
        assert_eq!(
            key_1, key_2,
            "two deserialized copies of the same JSON must produce the same cache key"
        );
    }

    #[test]
    fn cache_key_deterministic() {
        let config = GraphQueryConfig {
            handle: test_handle(),
            roots: Some(BTreeSet::from(["a".to_string(), "b".to_string()])),
            traversal: None,
        };
        assert_eq!(config.cache_key(), config.cache_key());
    }

    #[test]
    fn different_handles_produce_different_cache_keys() {
        let a = GraphQueryConfig {
            handle: "timeline_a".parse().unwrap(),
            roots: None,
            traversal: None,
        };
        let b = GraphQueryConfig {
            handle: "timeline_b".parse().unwrap(),
            roots: None,
            traversal: None,
        };
        assert_ne!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn roots_override_changes_cache_key() {
        let base = GraphQueryConfig {
            handle: test_handle(),
            roots: None,
            traversal: None,
        };
        let with_roots = GraphQueryConfig {
            handle: test_handle(),
            roots: Some(BTreeSet::from(["root_a".to_string()])),
            traversal: None,
        };
        assert_ne!(base.cache_key(), with_roots.cache_key());
    }

    #[test]
    fn explore_cache_key_format() {
        let key = ExploreCacheKey::from_hash(0x1a2b3c4d5e6f7890);
        assert_eq!(key.to_string(), "exp_1a2b3c4d5e6f7890");
    }

    /// Regression test for the giant-`tvc_delta` bug.
    ///
    /// Changing a single field (`max_tier`) on an inline TVC that carries a
    /// large `force_nodes` map must produce a tiny delta — not a copy of the
    /// whole `TraversalConfig`. Before enum field-level diffing, the
    /// `#[deltable(replace)]` on `TraversalOverride` serialized the entire
    /// inline config (~MBs) into the delta.
    #[test]
    fn max_tier_change_yields_tiny_delta() {
        use crate::traversal::tiered_traversal::AscendingTiersConfig;
        use crate::traversal::tiered_traversal::TieredTraversalConfig;

        // A large force_nodes map (the part that used to bloat the delta).
        let force_nodes: BTreeMap<String, Decision> = (0..5_000)
            .map(|i| (format!("SomeLongModuleName_{i:05}"), Decision::include()))
            .collect();

        let make = |max_tier: Option<usize>| GraphQueryConfig {
            handle: test_handle(),
            roots: None,
            traversal: Some(TraversalOverride::Inline(TraversalConfig {
                force_nodes: Some(force_nodes.clone()),
                tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(
                    AscendingTiersConfig {
                        tiers: vec![],
                        max_tier,
                    },
                )),
                ..Default::default()
            })),
        };

        let base = make(None);
        let modified = make(Some(1));

        // Sanity: the full configs really are large.
        let base_json_len = serde_json::to_string(&base).unwrap().len();
        assert!(
            base_json_len > 100_000,
            "expected a large base config, got {base_json_len} bytes"
        );

        let delta = base.derive_delta(&modified).expect("configs differ");
        let delta_json = serde_json::to_string(&delta).unwrap();

        // The delta must be tiny and must not drag along the force_nodes map.
        assert!(
            delta_json.len() < 500,
            "delta should be tiny, got {} bytes: {delta_json}",
            delta_json.len()
        );
        assert!(
            !delta_json.contains("force_nodes"),
            "delta must not contain force_nodes: {delta_json}"
        );
        assert!(
            !delta_json.contains("SomeLongModuleName"),
            "delta must not contain any node keys: {delta_json}"
        );

        // And it must still round-trip back to the modified config.
        let mut applied = base.clone();
        applied.apply_delta(delta).unwrap();
        assert_eq!(applied, modified);
    }
}

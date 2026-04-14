// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Query configuration types for graph exploration.
//!
//! `GraphQueryConfig` bundles roots, traversal settings, and a graph handle
//! into a single config for querying a graph.

use std::collections::BTreeSet;

use crate::traversal::TraversalConfig;
use crate::types::NodeName;

/// Configuration for querying a graph — roots to start from, traversal rules,
/// and which graph to query.
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Default,
    Clone,
    PartialEq,
    typegen::TypeGen,
    unigraph_delta::Deltable
)]
pub struct GraphQueryConfig {
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    #[serde(default)]
    pub roots: BTreeSet<NodeName>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub traversal_config: Option<TraversalConfig>,

    /// Graph target: timeline ID (`"my_timeline"`) for latest, or
    /// `"my_timeline~123"` for a specific snapshot.
    /// Uses the same format as `GraphKeyOrTimelineID`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::traversal::Decision;

    /// Constructs two identical `GraphQueryConfig`s by inserting fields in
    /// different order, then asserts they produce the same serialized key.
    ///
    /// This guards against non-deterministic containers (e.g. HashMap) sneaking
    /// into the type tree — if anyone replaces a BTreeMap/BTreeSet with a
    /// HashMap/HashSet, this test will start flaking.
    #[test]
    fn identical_configs_produce_identical_serialized_keys() {
        // Build config A: insert roots alphabetically
        let mut roots_a = BTreeSet::new();
        roots_a.insert("alpha".to_string());
        roots_a.insert("beta".to_string());
        roots_a.insert("gamma".to_string());

        let mut force_nodes_a = BTreeMap::new();
        force_nodes_a.insert("x".to_string(), Decision::include());
        force_nodes_a.insert("y".to_string(), Decision::exclude());
        force_nodes_a.insert("z".to_string(), Decision::include());

        let config_a = GraphQueryConfig {
            roots: roots_a,
            traversal_config: Some(TraversalConfig {
                force_nodes: Some(force_nodes_a),
                ..Default::default()
            }),
            handle: Some("my_timeline~42".to_string()),
        };

        // Build config B: insert roots in reverse order
        let mut roots_b = BTreeSet::new();
        roots_b.insert("gamma".to_string());
        roots_b.insert("alpha".to_string());
        roots_b.insert("beta".to_string());

        let mut force_nodes_b = BTreeMap::new();
        force_nodes_b.insert("z".to_string(), Decision::include());
        force_nodes_b.insert("x".to_string(), Decision::include());
        force_nodes_b.insert("y".to_string(), Decision::exclude());

        let config_b = GraphQueryConfig {
            roots: roots_b,
            traversal_config: Some(TraversalConfig {
                force_nodes: Some(force_nodes_b),
                ..Default::default()
            }),
            handle: Some("my_timeline~42".to_string()),
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
    /// serialized keys — this is the roundtrip that matters for LRU caches.
    #[test]
    fn deserialized_configs_produce_identical_serialized_keys() {
        let json = r#"{
            "roots": ["gamma", "alpha", "beta"],
            "traversal_config": {
                "force_nodes": {
                    "z": { "include": true },
                    "x": { "include": true },
                    "y": { "include": false }
                }
            },
            "handle": "my_timeline~42"
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
}

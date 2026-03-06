// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Test utilities for the Unigraph storage layer.
//!
//! Provides deterministic graph generation via [`TestGraphTimeline`],
//! formatting helpers for test snapshots, and graph equality assertions.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use unigraph_core::ArrayGraphNodes;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializableEdges;
use unigraph_core::ArrayGraphSerializableNodeMetadata;
use unigraph_core::NodeIDX;
// Re-export format_frames_table from unigraph_storage_core.
pub use unigraph_storage_core::format_frames_table;
use unigraph_storage_core::types::GraphID;

/// Deterministic graph generator for testing.
///
/// Uses an XORShift64 PRNG seeded by the graph ID to produce reproducible
/// graphs with varying numbers of nodes, edges, metrics, and tags.
pub struct TestGraphTimeline;

impl TestGraphTimeline {
    /// Generate the `id`-th graph in the test timeline.
    ///
    /// Deterministic: same `id` always produces the same graph.
    pub fn get_nth(id: u64) -> ArrayGraphSerializable {
        let mut rng = XorShift64::new(id.wrapping_mul(6364136223846793005).wrapping_add(1));

        // 3-50 nodes
        let node_count = 3 + (rng.next() % 48) as usize;

        let mut node_names_str = String::new();
        let mut node_name_offsets = vec![0usize];
        for i in 0..node_count {
            let name = format!("n_{:03}", i);
            node_names_str.push_str(&name);
            node_name_offsets.push(node_names_str.len());
        }

        // Directed edges: 0-3 per node (deduplicated via BTreeSet to
        // ensure delta round-trips produce identical graphs, since
        // apply_delta uses BTreeSet internally).
        let mut directed = Vec::new();
        let mut directed_offsets = vec![0usize];
        for _src in 0..node_count {
            let edge_count = rng.next() % 4;
            let mut targets = BTreeSet::new();
            for _ in 0..edge_count {
                let target = (rng.next() % node_count as u64) as usize;
                targets.insert(NodeIDX::from(target));
            }
            for target in targets {
                directed.push(target);
            }
            directed_offsets.push(directed.len());
        }

        // Metrics: always 3 metrics with random f32 values.
        // Using a fixed set of metric names ensures delta round-trips
        // produce identical results (no phantom all-zero metrics).
        let mut metrics = BTreeMap::new();
        for m in 0..3 {
            let metric_name = format!("metric_{}", m);
            let values: Vec<f32> = (0..node_count)
                .map(|_| {
                    let bits = rng.next() as u32;
                    // Generate a finite f32 in a reasonable range
                    (bits % 10000) as f32 / 100.0
                })
                .collect();
            metrics.insert(metric_name, values);
        }

        // Tagged edges: ~30% chance per node
        let mut tagged: BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<NodeIDX>>> = BTreeMap::new();
        for src in 0..node_count {
            if rng.next() % 100 < 30 {
                let tag = format!("tag_{}", rng.next() % 5);
                let target = NodeIDX::from((rng.next() % node_count as u64) as usize);
                tagged
                    .entry(NodeIDX::from(src))
                    .or_default()
                    .entry(tag)
                    .or_default()
                    .insert(target);
            }
        }

        // Tag sets: ~20% chance per node
        let mut tag_sets: BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
        for node in 0..node_count {
            if rng.next() % 100 < 20 {
                let set_name = format!("set_{}", rng.next() % 3);
                let tag_value = format!("value_{}", rng.next() % 10);
                tag_sets
                    .entry(NodeIDX::from(node))
                    .or_default()
                    .entry(set_name)
                    .or_default()
                    .insert(tag_value);
            }
        }

        ArrayGraphSerializable {
            node_names_ordered: Arc::new(ArrayGraphNodes::from_parts(
                node_names_str,
                node_name_offsets,
            )),
            edges: ArrayGraphSerializableEdges {
                directed,
                directed_offsets,
                tagged,
                dynamic: BTreeMap::new(),
            },
            node_metadata: ArrayGraphSerializableNodeMetadata { metrics, tag_sets },
            graph_settings: None,
            traversal_config: None,
            entry_points: None,
        }
    }
}

/// Minimal XORShift64 PRNG for deterministic test graph generation.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Ensure state is never zero
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

/// Format a list of blob keys for snapshot testing.
pub fn format_blob_keys(keys: &[String]) -> String {
    let mut sorted = keys.to_vec();
    sorted.sort();
    sorted.join("\n")
}

/// Assert two graphs are equal using their JSON representations.
///
/// Panics with a detailed line-by-line diff on failure.
pub fn assert_graphs_equal(a: &ArrayGraphSerializable, b: &ArrayGraphSerializable) {
    let a_json = serde_json::to_string_pretty(a).unwrap();
    let b_json = serde_json::to_string_pretty(b).unwrap();
    assert_eq!(a_json, b_json, "Graphs are not equal");
}

/// Helper to create a [`GraphTimeKey`] with a simple numeric timestamp.
pub fn make_graph_time_key(
    timeline: &str,
    graph_id: i64,
    seconds: i64,
) -> unigraph_storage_core::GraphTimeKey {
    use chrono::TimeZone;
    unigraph_storage_core::GraphTimeKey {
        timeline_id: unigraph_storage_core::TimelineID(timeline.to_string()),
        timestamp: chrono::Utc.timestamp_opt(seconds, 0).unwrap(),
        graph_id: GraphID(graph_id),
    }
}

/// Helper to create a [`GraphKey`].
pub fn make_graph_key(timeline: &str, graph_id: i64) -> unigraph_storage_core::GraphKey {
    unigraph_storage_core::GraphKey {
        timeline_id: unigraph_storage_core::TimelineID(timeline.to_string()),
        graph_id: GraphID(graph_id),
    }
}

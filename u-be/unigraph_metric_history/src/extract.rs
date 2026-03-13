// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Extract per-node metrics from an `ArrayGraphSerializable`.
//!
//! Graph metrics are stored as `BTreeMap<MetricName, Vec<f32>>` where each
//! `Vec<f32>` is indexed by `NodeIDX`. This module pivots that column-oriented
//! layout into a row-oriented `BTreeMap<NodeName, BTreeMap<MetricName, f64>>`.
//!
//! The f32→f64 conversion happens here so that `FlatHistory` can work with
//! f64 natively (future-proofing for f64 metrics). Nodes where ALL metrics
//! are zero are excluded — they carry no useful data and would just inflate
//! the history blobs.

use std::collections::BTreeMap;

use unigraph_core::ArrayGraphSerializable;

use crate::types::NodeMetricSnapshot;

/// Extract per-node metrics from an ArrayGraphSerializable.
///
/// Returns `BTreeMap<NodeName, NodeMetricSnapshot>` with f64 values.
/// Nodes where all metrics are zero are excluded (they carry no useful data).
pub fn extract_node_metrics(
    graph: &ArrayGraphSerializable,
) -> BTreeMap<String, NodeMetricSnapshot> {
    let nodes = &graph.node_names_ordered;
    let node_count = nodes.combined_nodes_len();
    let mut result = BTreeMap::new();

    for idx_raw in 0..node_count {
        let idx = unigraph_core::NodeIDX::from(idx_raw);
        let name = nodes.idx_to_name(idx);

        let mut snapshot = NodeMetricSnapshot::new();
        for (metric_name, values) in &graph.node_metadata.metrics {
            let val = values[idx_raw] as f64;
            if val != 0.0 {
                snapshot.insert(metric_name.clone(), val);
            }
        }

        if !snapshot.is_empty() {
            result.insert(name.to_string(), snapshot);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_from_test_graph() {
        let graph = unigraph_core::make_test_graph()
            .unwrap()
            .to_array_graph()
            .unwrap()
            .into_serializable();

        let metrics = extract_node_metrics(&graph);

        // The test graph should have some nodes with metrics.
        // We don't assert specific values since the test graph is deterministic
        // but we verify the extraction works and produces reasonable results.
        assert!(
            !metrics.is_empty() || graph.node_metadata.metrics.is_empty(),
            "if graph has metrics, extraction should find some non-zero nodes"
        );

        // All extracted values should be non-zero.
        for snapshot in metrics.values() {
            assert!(!snapshot.is_empty());
            for &val in snapshot.values() {
                assert_ne!(val, 0.0, "zero values should be excluded");
            }
        }
    }
}

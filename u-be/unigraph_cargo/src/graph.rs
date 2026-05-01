// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use unigraph_core::MapGraph;
use unigraph_core::graph_settings::GraphSettings;
use unigraph_core::graph_settings::MetricConfig;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricsConfig;
use unigraph_core::graph_settings::SizeFormatConfig;
use unigraph_core::graph_settings::SizeInputUnits;
use unigraph_core::graph_settings::SizeOutputUnits;
use unigraph_core::types::map_graph::GraphNode;

use crate::metadata::CargoGraph;
use crate::timings;

pub fn build_map_graph(
    cargo_graph: &CargoGraph,
    timings: Option<&BTreeMap<String, timings::UnitTiming>>,
    rlib_sizes: Option<&BTreeMap<String, f32>>,
) -> MapGraph {
    let mut nodes = BTreeMap::new();

    for (name, info) in &cargo_graph.crates {
        // Build directed edges (normal deps).
        let directed = if info.normal_deps.is_empty() {
            None
        } else {
            Some(info.normal_deps.iter().cloned().collect::<BTreeSet<_>>())
        };

        // Build tagged edges (dev + build deps).
        let mut tagged: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        if !info.dev_deps.is_empty() {
            tagged.insert("dev".to_string(), info.dev_deps.iter().cloned().collect());
        }
        if !info.build_deps.is_empty() {
            tagged.insert(
                "build".to_string(),
                info.build_deps.iter().cloned().collect(),
            );
        }
        let tagged = if tagged.is_empty() {
            None
        } else {
            Some(tagged)
        };

        // Build metrics.
        let mut metrics = BTreeMap::new();

        // Merge timing metrics if available.
        if let Some(timing_map) = timings {
            // Try matching by crate name (without version).
            let crate_base_name = info.name.split(" v").next().unwrap_or(&info.name);
            if let Some(timing) = timing_map.get(crate_base_name) {
                metrics.insert("build_time".to_string(), timing.duration);
                metrics.insert("rmeta_time".to_string(), timing.rmeta_time);
                metrics.insert("codegen_time".to_string(), timing.codegen_time);
            }
        }

        // Merge rlib size if available.
        if let Some(size_map) = rlib_sizes {
            let crate_base_name = info.name.split(" v").next().unwrap_or(&info.name);
            if let Some(&size) = size_map.get(crate_base_name) {
                metrics.insert("rlib_size".to_string(), size);
            }
        }

        // Build properties.
        let mut properties = BTreeMap::new();
        properties.insert("version".to_string(), info.version.clone());
        properties.insert("source".to_string(), info.source.clone());
        properties.insert("crate_type".to_string(), info.crate_type.clone());
        properties.insert("manifest_path".to_string(), info.manifest_path.clone());

        let node = GraphNode {
            properties: Some(properties),
            labels: None,
            metrics: if metrics.is_empty() {
                None
            } else {
                Some(metrics)
            },
            edges_directed: directed,
            edges_tagged: tagged,
            edges_dynamic: None,
        };

        nodes.insert(name.clone(), node);
    }

    let entry_points = if cargo_graph.workspace_members.is_empty() {
        None
    } else {
        Some(
            cargo_graph
                .workspace_members
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
        )
    };

    let time_format = MetricFormat::NumberWithVariablePrecision {
        min_precision: Some(1),
        max_precision: Some(2),
        use_delimiter: None,
    };

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "rlib_size".to_string(),
        MetricConfig {
            self_view: None,
            transitive: None,
            dominated: None,
            tiered: None,
            tiered_dominated: None,
            format: Some(MetricFormat::Size(SizeFormatConfig {
                input_units: SizeInputUnits::Bytes,
                output_units: SizeOutputUnits::MB,
                min_precision: None,
                max_precision: Some(2),
                use_delimiter: None,
            })),
            description: Some("Compiled .rlib artifact size".to_string()),
        },
    );
    metrics.insert(
        "build_time".to_string(),
        MetricConfig {
            self_view: None,
            transitive: None,
            dominated: None,
            tiered: None,
            tiered_dominated: None,
            format: Some(time_format.clone()),
            description: Some("Wall-clock build duration".to_string()),
        },
    );
    metrics.insert(
        "rmeta_time".to_string(),
        MetricConfig {
            self_view: None,
            transitive: None,
            dominated: None,
            tiered: None,
            tiered_dominated: None,
            format: Some(time_format.clone()),
            description: Some(
                "Time to produce rmeta (enables downstream crates to start building)".to_string(),
            ),
        },
    );
    metrics.insert(
        "codegen_time".to_string(),
        MetricConfig {
            self_view: None,
            transitive: None,
            dominated: None,
            tiered: None,
            tiered_dominated: None,
            format: Some(time_format),
            description: Some("Time spent in codegen phase".to_string()),
        },
    );

    let graph_settings = Some(GraphSettings {
        description: None,
        metrics_config: Some(MetricsConfig {
            default_availability: None,
            default_visibility: None,
            metrics: Some(metrics),
            parents_count: None,
            count_transitive: None,
            count_dominated: None,
        }),
        metrics_visibility: None,
        ui_settings: None,
    });

    MapGraph {
        nodes,
        traversal_config: None,
        graph_settings,
        entry_points,
        properties: BTreeMap::new(),
    }
}

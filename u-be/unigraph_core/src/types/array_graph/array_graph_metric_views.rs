// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use crate::ArrayGraph;
use crate::MetricView;
use crate::types::array_graph::graph_settings::Availability;
use crate::types::array_graph::graph_settings::DefaultAvailability;
use crate::types::array_graph::graph_settings::GraphStructure;
use crate::types::array_graph::graph_settings::MetricViewVisibility;
use crate::types::array_graph::graph_settings::MetricsConfig;

// ── Public API ──────────────────────────────────────────────

/// All metric views that are AVAILABLE in this graph (Layer 1).
///
/// Filters by `MetricsConfig.default_availability` + per-metric overrides.
/// When no config is present, all views are available (backward compat).
pub fn available_metric_views(ag: &ArrayGraph) -> Vec<MetricView> {
    let tier_names = collect_tier_names(ag);
    let config = metrics_config(ag);
    let mut views = Vec::new();

    for metric_name in ag.data.node_metadata.metrics.keys() {
        push_available_base_views(&mut views, metric_name, config);
        push_available_tiered_views(&mut views, metric_name, &tier_names, config);
    }

    push_available_structural_counts(&mut views, config);
    if !tier_names.is_empty() {
        views.push(MetricView::TierIndex {});
    }
    views
}

/// Of the available views, which are visible for the given graph structure?
///
/// Applies `MetricsConfig.default_visibility` + per-view overrides from
/// `GraphSettings.metrics_visibility`. Dominated views default to visible
/// only in `Dominator` mode.
pub fn visible_metric_views(ag: &ArrayGraph, structure: GraphStructure) -> Vec<MetricView> {
    let available = available_metric_views(ag);
    let config = metrics_config(ag);
    let overrides = metrics_visibility(ag);
    available
        .into_iter()
        .filter(|view| is_view_visible(view, config, overrides, structure))
        .collect()
}

/// Deprecated: use `available_metric_views()` or `visible_metric_views()`.
pub fn enabled_metric_views(ag: &ArrayGraph) -> Vec<MetricView> {
    available_metric_views(ag)
}

// ── Accessors ───────────────────────────────────────────────

fn collect_tier_names(ag: &ArrayGraph) -> Vec<&str> {
    ag.runtime
        .state
        .tiers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect()
}

fn metrics_config(ag: &ArrayGraph) -> Option<&MetricsConfig> {
    ag.data
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.metrics_config.as_ref())
}

fn metrics_visibility(ag: &ArrayGraph) -> Option<&BTreeMap<String, MetricViewVisibility>> {
    ag.data
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.metrics_visibility.as_ref())
}

// ── Visibility resolution ───────────────────────────────────

fn is_view_visible(
    view: &MetricView,
    config: Option<&MetricsConfig>,
    overrides: Option<&BTreeMap<String, MetricViewVisibility>>,
    structure: GraphStructure,
) -> bool {
    let explicit = overrides.and_then(|m| m.get(&view.to_string()));
    if let Some(vis) = explicit {
        return resolve_visibility(*vis, structure);
    }
    let default_vis = view_type_default_visibility(view, config);
    resolve_visibility(default_vis, structure)
}

fn resolve_visibility(vis: MetricViewVisibility, structure: GraphStructure) -> bool {
    match vis {
        MetricViewVisibility::Enabled => true,
        MetricViewVisibility::EnabledInDominatorMode => structure == GraphStructure::Dominator,
        MetricViewVisibility::Hidden => false,
    }
}

fn view_type_default_visibility(
    view: &MetricView,
    config: Option<&MetricsConfig>,
) -> MetricViewVisibility {
    let Some(dv) = config.and_then(|c| c.default_visibility.as_ref()) else {
        return hardcoded_default_visibility(view);
    };
    let per_type = match view {
        MetricView::Metric { .. } => dv.self_view,
        MetricView::Transitive { .. } | MetricView::CountTransitive {} => dv.transitive,
        MetricView::Dominated { .. } | MetricView::CountDominated {} => dv.dominated,
        MetricView::Tiered { .. } => dv.tiered,
        MetricView::TieredDominated { .. } => dv.tiered_dominated,
        MetricView::ParentsCount {} | MetricView::TierIndex {} => None,
    };
    per_type
        .or(dv.all)
        .unwrap_or_else(|| hardcoded_default_visibility(view))
}

fn hardcoded_default_visibility(view: &MetricView) -> MetricViewVisibility {
    if view.is_dominated() {
        MetricViewVisibility::EnabledInDominatorMode
    } else {
        MetricViewVisibility::Enabled
    }
}

// ── Availability-filtered view generation ───────────────────

fn resolve_available(
    config: Option<&MetricsConfig>,
    name: &str,
    per_metric: fn(
        &crate::types::array_graph::graph_settings::MetricConfig,
    ) -> Option<Availability>,
    default_field: fn(&DefaultAvailability) -> Option<Availability>,
) -> bool {
    let Some(cfg) = config else {
        return true;
    };
    cfg.resolve_availability(name, per_metric, default_field)
        .is_available()
}

fn push_available_base_views(
    views: &mut Vec<MetricView>,
    name: &str,
    config: Option<&MetricsConfig>,
) {
    if resolve_available(config, name, |mc| mc.self_view, |d| d.self_view) {
        views.push(MetricView::Metric {
            name: name.to_string(),
        });
    }
    if resolve_available(config, name, |mc| mc.transitive, |d| d.transitive) {
        views.push(MetricView::Transitive {
            name: name.to_string(),
        });
    }
    if resolve_available(config, name, |mc| mc.dominated, |d| d.dominated) {
        views.push(MetricView::Dominated {
            name: name.to_string(),
        });
    }
}

fn push_available_tiered_views(
    views: &mut Vec<MetricView>,
    name: &str,
    tier_names: &[&str],
    config: Option<&MetricsConfig>,
) {
    let tiered = resolve_available(config, name, |mc| mc.tiered, |d| d.tiered);
    let tiered_dom = resolve_available(
        config,
        name,
        |mc| mc.tiered_dominated,
        |d| d.tiered_dominated,
    );

    for &tier_name in tier_names {
        if tiered {
            views.push(MetricView::Tiered {
                name: name.to_string(),
                tier_name: tier_name.to_string(),
            });
        }
        if tiered_dom {
            views.push(MetricView::TieredDominated {
                name: name.to_string(),
                tier_name: tier_name.to_string(),
            });
        }
    }
}

fn push_available_structural_counts(views: &mut Vec<MetricView>, config: Option<&MetricsConfig>) {
    let resolve_structural = |getter: fn(&MetricsConfig) -> Option<Availability>| -> bool {
        config
            .and_then(getter)
            .unwrap_or(Availability::Available)
            .is_available()
    };

    if resolve_structural(|c| c.parents_count) {
        views.push(MetricView::ParentsCount {});
    }
    if resolve_structural(|c| c.count_transitive) {
        views.push(MetricView::CountTransitive {});
    }
    if resolve_structural(|c| c.count_dominated) {
        views.push(MetricView::CountDominated {});
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k9::snapshot;

    use crate::ArrayGraph;
    use crate::types::array_graph::graph_settings::Availability;
    use crate::types::array_graph::graph_settings::DefaultAvailability;
    use crate::types::array_graph::graph_settings::DefaultVisibility;
    use crate::types::array_graph::graph_settings::GraphSettings;
    use crate::types::array_graph::graph_settings::GraphStructure;
    use crate::types::array_graph::graph_settings::MetricConfig;
    use crate::types::array_graph::graph_settings::MetricViewVisibility;
    use crate::types::array_graph::graph_settings::MetricsConfig;
    use crate::types::map_graph::MapGraph;

    const GRAPH_WITH_TIERS: &str = r#"{
        "nodes": {
            "app":  { "edges_directed": ["lib"], "metrics": { "size": 100, "lines": 50 } },
            "lib":  { "metrics": { "size": 200, "lines": 80 } }
        },
        "traversal_config": {
            "tiered_traversal": {
                "AscendingTiers": {
                    "tiers": [
                        { "name": "eager", "tags_that_transition_to_this_tier": [] },
                        { "name": "lazy",  "tags_that_transition_to_this_tier": ["lazy"] }
                    ]
                }
            }
        }
    }"#;

    const SIMPLE_GRAPH: &str = r#"{
        "nodes": {
            "a": { "edges_directed": ["b"], "metrics": { "size": 10 } },
            "b": { "metrics": { "size": 20 } }
        }
    }"#;

    fn make_graph(json: &str) -> ArrayGraph {
        MapGraph::from_json(json)
            .unwrap()
            .to_array_graph(&ll::Task::create_new("test"))
            .unwrap()
    }

    fn format_views(views: &[crate::MetricView]) -> String {
        views
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn with_settings(mut ag: ArrayGraph, gs: GraphSettings) -> ArrayGraph {
        ag.data.graph_settings = Some(gs);
        ag
    }

    // ── Available views ─────────────────────────────────────

    #[test]
    fn available_no_config() {
        let ag = make_graph(SIMPLE_GRAPH);
        snapshot!(
            format_views(&ag.available_metric_views()),
            "
size
size~transitive
size~dominated
parents-count
node-count~transitive
node-count~dominated
"
        );
    }

    #[test]
    fn available_with_tiers() {
        let ag = make_graph(GRAPH_WITH_TIERS);
        snapshot!(
            format_views(&ag.available_metric_views()),
            "
lines
lines~transitive
lines~dominated
lines#eager
lines#eager~dominated
lines#lazy
lines#lazy~dominated
size
size~transitive
size~dominated
size#eager
size#eager~dominated
size#lazy
size#lazy~dominated
parents-count
node-count~transitive
node-count~dominated
tier
"
        );
    }

    #[test]
    fn available_per_metric_unavailable() {
        let ag = with_settings(
            make_graph(GRAPH_WITH_TIERS),
            GraphSettings {
                description: None,
                metrics_config: Some(MetricsConfig {
                    default_availability: None,
                    default_visibility: None,
                    metrics: Some(BTreeMap::from([(
                        "lines".to_string(),
                        MetricConfig {
                            self_view: None,
                            transitive: None,
                            dominated: None,
                            tiered: Some(Availability::Unavailable),
                            tiered_dominated: Some(Availability::Unavailable),
                            format: None,
                            description: None,
                        },
                    )])),
                    parents_count: None,
                    count_transitive: None,
                    count_dominated: None,
                }),
                metrics_visibility: None,
                ui_settings: None,
            },
        );
        snapshot!(
            format_views(&ag.available_metric_views()),
            "
lines
lines~transitive
lines~dominated
size
size~transitive
size~dominated
size#eager
size#eager~dominated
size#lazy
size#lazy~dominated
parents-count
node-count~transitive
node-count~dominated
tier
"
        );
    }

    #[test]
    fn available_default_tiered_unavailable() {
        let ag = with_settings(
            make_graph(GRAPH_WITH_TIERS),
            GraphSettings {
                description: None,
                metrics_config: Some(MetricsConfig {
                    default_availability: Some(DefaultAvailability {
                        self_view: None,
                        transitive: None,
                        dominated: None,
                        tiered: Some(Availability::Unavailable),
                        tiered_dominated: Some(Availability::Unavailable),
                    }),
                    default_visibility: None,
                    metrics: None,
                    parents_count: None,
                    count_transitive: None,
                    count_dominated: None,
                }),
                metrics_visibility: None,
                ui_settings: None,
            },
        );
        snapshot!(
            format_views(&ag.available_metric_views()),
            "
lines
lines~transitive
lines~dominated
size
size~transitive
size~dominated
parents-count
node-count~transitive
node-count~dominated
tier
"
        );
    }

    // ── Visible views ───────────────────────────────────────

    #[test]
    fn visible_no_config_forward() {
        let ag = make_graph(SIMPLE_GRAPH);
        snapshot!(
            format_views(&ag.visible_metric_views(GraphStructure::Forward)),
            "
size
size~transitive
parents-count
node-count~transitive
"
        );
    }

    #[test]
    fn visible_no_config_dominator() {
        let ag = make_graph(SIMPLE_GRAPH);
        snapshot!(
            format_views(&ag.visible_metric_views(GraphStructure::Dominator)),
            "
size
size~transitive
size~dominated
parents-count
node-count~transitive
node-count~dominated
"
        );
    }

    #[test]
    fn visible_tiered_hidden_by_default() {
        let ag = with_settings(
            make_graph(GRAPH_WITH_TIERS),
            GraphSettings {
                description: None,
                metrics_config: Some(MetricsConfig {
                    default_availability: None,
                    default_visibility: Some(DefaultVisibility {
                        all: None,
                        self_view: None,
                        transitive: None,
                        dominated: None,
                        tiered: Some(MetricViewVisibility::Hidden),
                        tiered_dominated: Some(MetricViewVisibility::Hidden),
                    }),
                    metrics: None,
                    parents_count: None,
                    count_transitive: None,
                    count_dominated: None,
                }),
                metrics_visibility: None,
                ui_settings: None,
            },
        );
        snapshot!(
            format_views(&ag.visible_metric_views(GraphStructure::Forward)),
            "
lines
lines~transitive
size
size~transitive
parents-count
node-count~transitive
tier
"
        );
    }

    #[test]
    fn visible_all_hidden() {
        let ag = with_settings(
            make_graph(SIMPLE_GRAPH),
            GraphSettings {
                description: None,
                metrics_config: Some(MetricsConfig {
                    default_availability: None,
                    default_visibility: Some(DefaultVisibility {
                        all: Some(MetricViewVisibility::Hidden),
                        self_view: None,
                        transitive: None,
                        dominated: None,
                        tiered: None,
                        tiered_dominated: None,
                    }),
                    metrics: None,
                    parents_count: None,
                    count_transitive: None,
                    count_dominated: None,
                }),
                metrics_visibility: None,
                ui_settings: None,
            },
        );
        let views = ag.visible_metric_views(GraphStructure::Forward);
        assert!(views.is_empty(), "all views hidden");
    }

    #[test]
    fn visible_all_hidden_with_type_override() {
        let ag = with_settings(
            make_graph(SIMPLE_GRAPH),
            GraphSettings {
                description: None,
                metrics_config: Some(MetricsConfig {
                    default_availability: None,
                    default_visibility: Some(DefaultVisibility {
                        all: Some(MetricViewVisibility::Hidden),
                        self_view: None,
                        transitive: Some(MetricViewVisibility::Enabled),
                        dominated: None,
                        tiered: None,
                        tiered_dominated: None,
                    }),
                    metrics: None,
                    parents_count: None,
                    count_transitive: None,
                    count_dominated: None,
                }),
                metrics_visibility: None,
                ui_settings: None,
            },
        );
        snapshot!(
            format_views(&ag.visible_metric_views(GraphStructure::Forward)),
            "
size~transitive
node-count~transitive
"
        );
    }

    #[test]
    fn visible_per_view_override_beats_default() {
        let mut overrides = BTreeMap::new();
        overrides.insert("size#eager".to_string(), MetricViewVisibility::Enabled);

        let ag = with_settings(
            make_graph(GRAPH_WITH_TIERS),
            GraphSettings {
                description: None,
                metrics_config: Some(MetricsConfig {
                    default_availability: None,
                    default_visibility: Some(DefaultVisibility {
                        all: Some(MetricViewVisibility::Hidden),
                        self_view: None,
                        transitive: None,
                        dominated: None,
                        tiered: None,
                        tiered_dominated: None,
                    }),
                    metrics: None,
                    parents_count: None,
                    count_transitive: None,
                    count_dominated: None,
                }),
                metrics_visibility: Some(overrides),
                ui_settings: None,
            },
        );
        snapshot!(
            format_views(&ag.visible_metric_views(GraphStructure::Forward)),
            "size#eager"
        );
    }

    #[test]
    fn visible_dominated_shows_in_dominator_mode() {
        let ag = make_graph(GRAPH_WITH_TIERS);
        let forward = ag.visible_metric_views(GraphStructure::Forward);
        let dominator = ag.visible_metric_views(GraphStructure::Dominator);

        assert!(
            !forward.iter().any(|v| v.to_string().contains("dominated")),
            "no dominated views in forward mode"
        );
        assert!(
            dominator
                .iter()
                .any(|v| v.to_string().contains("dominated")),
            "dominated views show in dominator mode"
        );
    }

    #[test]
    fn format_inherited_by_derived_views() {
        use crate::graph_settings::MetricFormat;
        use crate::graph_settings::SizeFormatConfig;
        use crate::graph_settings::SizeInputUnits;
        use crate::graph_settings::SizeOutputUnits;

        let size_format = MetricFormat::Size(SizeFormatConfig {
            input_units: SizeInputUnits::Bytes,
            output_units: SizeOutputUnits::MB,
            min_precision: None,
            max_precision: Some(2),
            use_delimiter: None,
        });

        let config = MetricsConfig {
            default_availability: None,
            default_visibility: None,
            metrics: Some(BTreeMap::from([(
                "size".to_string(),
                MetricConfig {
                    self_view: None,
                    transitive: None,
                    dominated: None,
                    tiered: None,
                    tiered_dominated: None,
                    format: Some(size_format.clone()),
                    description: Some("File size".to_string()),
                },
            )])),
            parents_count: None,
            count_transitive: None,
            count_dominated: None,
        };

        let views = vec![
            (
                "size",
                crate::MetricView::Metric {
                    name: "size".into(),
                },
            ),
            (
                "size~transitive",
                crate::MetricView::Transitive {
                    name: "size".into(),
                },
            ),
            (
                "size~dominated",
                crate::MetricView::Dominated {
                    name: "size".into(),
                },
            ),
            (
                "size#T1",
                crate::MetricView::Tiered {
                    name: "size".into(),
                    tier_name: "T1".into(),
                },
            ),
            (
                "size#T1~dominated",
                crate::MetricView::TieredDominated {
                    name: "size".into(),
                    tier_name: "T1".into(),
                },
            ),
            ("parents-count", crate::MetricView::ParentsCount {}),
        ];

        let mut results = Vec::new();
        for (label, view) in &views {
            let fmt = config.format_for_view(view);
            results.push(format!(
                "{:<20} {}",
                label,
                if fmt.is_some() { "Size(MB)" } else { "-" }
            ));
        }

        snapshot!(
            results.join("\n"),
            "
size                 Size(MB)
size~transitive      Size(MB)
size~dominated       Size(MB)
size#T1              Size(MB)
size#T1~dominated    Size(MB)
parents-count        -
"
        );
    }
}

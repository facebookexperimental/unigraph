// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use crate::ArrayGraph;
use crate::MetricView;
use crate::types::array_graph::graph_settings::Availability;
use crate::types::array_graph::graph_settings::DefaultAvailability;
use crate::types::array_graph::graph_settings::DefaultVisibility;
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
        MetricView::ParentsCount {} => None,
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

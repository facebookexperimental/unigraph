// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use crate::ArrayGraph;
use crate::MetricView;
use crate::types::array_graph::graph_settings::MetricViewSettings;
use crate::types::array_graph::graph_settings::MetricViewVisibility;

/// Enumerate every combination of metric name × view type
/// (plain, transitive, dominated, tiered per tier) plus the
/// structural counts (parents, transitive count, dominated count).
fn all_metric_views(ag: &ArrayGraph) -> Vec<MetricView> {
    let tier_names = collect_tier_names(ag);
    let mut views = Vec::new();

    for metric_name in ag.data.node_metadata.metrics.keys() {
        push_base_views(&mut views, metric_name);
        push_tiered_views(&mut views, metric_name, &tier_names);
    }

    push_structural_counts(&mut views);
    views
}

/// Filter `available_metric_views` through `graph_settings.metric_settings`
/// visibility. Views with no explicit setting are kept (default = enabled).
pub fn enabled_metric_views(ag: &ArrayGraph) -> Vec<MetricView> {
    let all = all_metric_views(ag);
    let Some(settings) = metric_settings(ag) else {
        return all;
    };
    all.into_iter()
        .filter(|view| is_view_enabled(view, settings))
        .collect()
}

// ── Helpers ──────────────────────────────────────────────────

fn collect_tier_names(ag: &ArrayGraph) -> Vec<&str> {
    ag.runtime
        .state
        .tiers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect()
}

fn push_base_views(views: &mut Vec<MetricView>, name: &str) {
    views.push(MetricView::Metric {
        name: name.to_string(),
    });
    views.push(MetricView::Transitive {
        name: name.to_string(),
    });
    views.push(MetricView::Dominated {
        name: name.to_string(),
    });
}

fn push_tiered_views(views: &mut Vec<MetricView>, name: &str, tier_names: &[&str]) {
    for &tier_name in tier_names {
        views.push(MetricView::Tiered {
            name: name.to_string(),
            tier_name: tier_name.to_string(),
        });
        views.push(MetricView::TieredDominated {
            name: name.to_string(),
            tier_name: tier_name.to_string(),
        });
        views.push(MetricView::ConjointTiered {
            name: name.to_string(),
            tier_name: tier_name.to_string(),
        });
    }
}

fn push_structural_counts(views: &mut Vec<MetricView>) {
    views.push(MetricView::ParentsCount {});
    views.push(MetricView::CountTransitive {});
    views.push(MetricView::CountDominated {});
    views.push(MetricView::CountConjoint {});
}

fn metric_settings(ag: &ArrayGraph) -> Option<&BTreeMap<String, MetricViewSettings>> {
    ag.data
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.ui_settings.as_ref())
        .and_then(|ui| ui.columns.as_ref())
        .and_then(|cols| cols.metric_settings.as_ref())
}

fn is_view_enabled(view: &MetricView, settings: &BTreeMap<String, MetricViewSettings>) -> bool {
    let key = view.to_string();
    match settings.get(key.as_str()) {
        Some(s) => !matches!(
            s.visibility,
            Some(MetricViewVisibility::Hidden {} | MetricViewVisibility::Unavailable { .. })
        ),
        None => true,
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Which metric columns a delta view shows, and what each cell contains.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use anyhow::bail;
use unigraph_core::GraphSide;
use unigraph_core::MetricColumn;
use unigraph_core::MetricSide;
use unigraph_core::MetricView;
use unigraph_core::NodeIDX;
use unigraph_core::TwinGraph;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricsConfig;

/// Resolve the column list.
///
/// `None` → the default delta view: for every metric view visible in this
/// graph, the right-hand value and its `∆`. `Some(list)` → exactly that list,
/// validated against the views this graph actually has.
pub fn resolve_columns(
    tg: &TwinGraph,
    requested: &Option<Vec<MetricColumn>>,
    structure: GraphStructure,
) -> Result<Vec<MetricColumn>> {
    match requested {
        None => Ok(default_columns(tg, structure)),
        Some(list) => validate_columns(tg, list),
    }
}

/// Right value + `∆` for each visible view.
fn default_columns(tg: &TwinGraph, structure: GraphStructure) -> Vec<MetricColumn> {
    let metrics_config =
        tg.r.graph_settings()
            .and_then(|gs| gs.metrics_config.as_ref());

    tg.r.visible_metric_views(structure)
        .into_iter()
        .flat_map(|view| {
            let delta = has_meaningful_delta(&view, metrics_config)
                .then(|| MetricColumn::new(view.clone(), MetricSide::Delta));
            [Some(MetricColumn::new(view, MetricSide::Right)), delta]
        })
        .flatten()
        .collect()
}

/// Categorical views get no `∆` column. Subtracting two tier indices or two
/// enum codes yields a number that formats back into a nonsense label —
/// `node_type` going `root` → `root` would render its zero delta as "root".
/// The UI reaches the same conclusion: `NodeTierColumn`, `EnumMetricColumn`,
/// and `TimespanMetricColumn` each render one column with no `∆` sibling.
pub fn has_meaningful_delta(view: &MetricView, metrics_config: Option<&MetricsConfig>) -> bool {
    if matches!(view, MetricView::TierIndex {}) {
        return false;
    }
    !matches!(
        metrics_config.and_then(|config| config.format_for_view(view)),
        Some(MetricFormat::Enum { .. } | MetricFormat::TimespanStart { .. })
    )
}

fn validate_columns(tg: &TwinGraph, list: &[MetricColumn]) -> Result<Vec<MetricColumn>> {
    let available: BTreeSet<String> =
        tg.r.available_metric_views()
            .iter()
            .chain(tg.l.available_metric_views().iter())
            .map(|v| v.to_string())
            .collect();

    let invalid: Vec<String> = list
        .iter()
        .filter(|tmv| !available.contains(&tmv.view.to_string()))
        .map(|tmv| tmv.to_string())
        .collect();

    if !invalid.is_empty() {
        bail!(
            "Unknown metric view(s): {}. Available views:\n{}",
            invalid.join(", "),
            available
                .iter()
                .map(|v| format!("  - {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    Ok(list.to_vec())
}

/// The flat metrics map for one merged node, keyed by column name.
pub fn build_map(
    tg: &TwinGraph,
    merged_idx: NodeIDX,
    columns: &[MetricColumn],
) -> Result<BTreeMap<String, f64>> {
    columns
        .iter()
        .map(|column| Ok((column.to_string(), compute(tg, merged_idx, column)?)))
        .collect()
}

pub fn compute(tg: &TwinGraph, merged_idx: NodeIDX, column: &MetricColumn) -> Result<f64> {
    match column.side {
        MetricSide::Left => side_value(tg, GraphSide::Left, merged_idx, &column.view),
        MetricSide::Right => side_value(tg, GraphSide::Right, merged_idx, &column.view),
        MetricSide::Delta => delta_value(tg, merged_idx, &column.view),
    }
}

/// A node that doesn't exist on a side contributes `0.0` there, so an added
/// node's `∆` is its full right-hand value and a removed one's is negative.
fn side_value(
    tg: &TwinGraph,
    side: GraphSide,
    merged_idx: NodeIDX,
    view: &MetricView,
) -> Result<f64> {
    match tg.to_local(side, merged_idx) {
        Some(local_idx) => tg.graph(side).metric_value(local_idx, view),
        None => Ok(0.0),
    }
}

/// `∆` is deliberately **not** a plain subtraction for every view — it mirrors
/// what the UI's delta columns show.
///
/// Tiered and transitive-node-count deltas go through `TwinGraph`'s *exclusive*
/// helpers, which walk both sides skipping nodes that didn't change. That way a
/// new node whose dependencies already existed reports what it actually added
/// rather than its entire subtree. See `twin_graph/metrics.rs` for the worked
/// example. Everything else is `R - L`, matching `MetricDeltaViewColumn` and
/// `TransitiveMetricDeltaColumn` in `u-fe/tree_table/columns/metrics.tsx`.
fn delta_value(tg: &TwinGraph, merged_idx: NodeIDX, view: &MetricView) -> Result<f64> {
    match view {
        MetricView::Tiered { name, tier_name } => Ok(*tg
            .get_transitive_tiered_delta(merged_idx, name)?
            .get(tier_name.as_str())
            .unwrap_or(&0.0)),
        MetricView::CountTransitive {} => Ok(tg.get_transitive_count_delta(merged_idx)? as f64),
        _ => {
            let right = side_value(tg, GraphSide::Right, merged_idx, view)?;
            let left = side_value(tg, GraphSide::Left, merged_idx, view)?;
            Ok(right - left)
        }
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::GraphSettings;
use unigraph_core::MetricView;
use unigraph_rpc::RpcExec;

use crate::Unigraph;
use crate::graph_handle::GraphHandle;
use crate::graph_handle::resolve_graph_handle;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct AboutGraphInput {
    /// Graph handle: a timeline_id ("cargo"), graph_key ("cargo~356"),
    /// or gqc_key ("gqc_abc123").
    pub handle: GraphHandle,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct AboutGraphOutput {
    /// Graph description from settings, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Graph statistics (node/edge counts by kind, tier names, etc).
    pub stats: unigraph_core::ArrayGraphStats,

    /// Per-metric info: description + list of derived views.
    pub metrics: Vec<AboutGraphMetricInfo>,

    /// All available metric views (flat list).
    pub metric_views: Vec<String>,

    /// Graph-level settings (description, UI config), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_settings: Option<GraphSettings>,

    /// Graph-level key-value properties.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,

    /// Human-readable markdown summary of the graph.
    /// Optimized for LLM consumption — use this field to understand the graph
    /// before exploring it with ExploreGraph.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct AboutGraphMetricInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub derived_views: Vec<String>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for AboutGraphInput {
    type Output = AboutGraphOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<AboutGraphOutput> {
        let ttl = Duration::from_secs(5 * 60);
        let ag = resolve_graph_handle(&self.handle, ctx, task, ttl).await?;
        let handle_str = self.handle.to_string();
        tokio::task::spawn_blocking(move || build_about(&ag, &handle_str)).await?
    }
}

// ── Build output ────────────────────────────────────────────

fn build_about(ag: &Arc<ArrayGraph>, handle: &str) -> Result<AboutGraphOutput> {
    let stats = ag.stats();
    let description = extract_description(ag);
    let available = ag.available_metric_views();
    let metrics = collect_metric_infos(ag, &available);
    let metric_views: Vec<String> = available.iter().map(|v| v.to_string()).collect();
    let graph_settings = ag.graph_settings().cloned();
    let properties = ag.data.properties.clone();
    let text = render_markdown(
        handle,
        description.as_deref(),
        &stats,
        &metrics,
        &metric_views,
    );

    Ok(AboutGraphOutput {
        description,
        stats,
        metrics,
        metric_views,
        graph_settings,
        properties,
        text,
    })
}

fn extract_description(ag: &ArrayGraph) -> Option<String> {
    ag.graph_settings().and_then(|gs| gs.description.clone())
}

fn collect_metric_infos(ag: &ArrayGraph, available: &[MetricView]) -> Vec<AboutGraphMetricInfo> {
    let metrics_config = ag
        .graph_settings()
        .and_then(|gs| gs.metrics_config.as_ref());

    let metric_names: Vec<&str> = ag
        .data
        .node_metadata
        .metrics
        .keys()
        .map(|s| s.as_str())
        .collect();

    metric_names
        .iter()
        .map(|&name| {
            let description =
                metrics_config.and_then(|mc| mc.description_for(name).map(|s| s.to_string()));
            let derived_views = available
                .iter()
                .filter(|v| {
                    v.metric_name() == Some(name) && !matches!(v, MetricView::Metric { .. })
                })
                .map(|v| v.to_string())
                .collect();
            AboutGraphMetricInfo {
                name: name.to_string(),
                description,
                derived_views,
            }
        })
        .collect()
}

// ── Markdown rendering ──────────────────────────────────────

fn render_markdown(
    handle: &str,
    description: Option<&str>,
    stats: &unigraph_core::ArrayGraphStats,
    metrics: &[AboutGraphMetricInfo],
    metric_views: &[String],
) -> String {
    let mut out = String::with_capacity(512);

    write_heading(&mut out, handle);
    write_description(&mut out, description);
    write_stats(&mut out, stats);
    write_metrics(&mut out, metrics);
    write_all_metric_views(&mut out, metric_views);
    write_tiers(&mut out, &stats.tier_names);

    out
}

fn write_heading(out: &mut String, handle: &str) {
    let _ = writeln!(out, "# Graph: {handle}");
}

fn write_description(out: &mut String, description: Option<&str>) {
    if let Some(desc) = description {
        let _ = writeln!(out, "\n{desc}");
    }
}

fn write_stats(out: &mut String, stats: &unigraph_core::ArrayGraphStats) {
    let _ = writeln!(out, "\n## Stats\n");
    let _ = writeln!(out, "- **Nodes**: {}", stats.num_all_nodes);
    let _ = write!(
        out,
        "- **Edges**: {} ({} directed",
        stats.num_all_edges, stats.num_directed_edges,
    );
    if stats.num_tagged_edges > 0 {
        let _ = write!(out, ", {} tagged", stats.num_tagged_edges);
    }
    if stats.num_dynamic_edges > 0 {
        let _ = write!(out, ", {} dynamic", stats.num_dynamic_edges);
    }
    out.push_str(")\n");

    if stats.num_unreachable_nodes > 0 {
        let _ = writeln!(
            out,
            "- **Unreachable nodes**: {}",
            stats.num_unreachable_nodes
        );
    }
    if stats.num_excluded_edges > 0 {
        let _ = writeln!(out, "- **Excluded edges**: {}", stats.num_excluded_edges);
    }
}

fn write_metrics(out: &mut String, metrics: &[AboutGraphMetricInfo]) {
    if metrics.is_empty() {
        return;
    }

    let _ = writeln!(out, "\n## Metrics\n");
    for metric in metrics {
        if let Some(desc) = &metric.description {
            let _ = writeln!(out, "- **`{}`** — {desc}", metric.name);
        } else {
            let _ = writeln!(out, "- **`{}`**", metric.name);
        }
    }
}

fn write_all_metric_views(out: &mut String, metric_views: &[String]) {
    if metric_views.is_empty() {
        return;
    }

    let _ = writeln!(out, "\n## All Available Metric Views\n");
    for view in metric_views {
        let _ = writeln!(out, "- `{view}`");
    }
}

fn write_tiers(out: &mut String, tier_names: &[String]) {
    if tier_names.is_empty() {
        return;
    }

    let _ = writeln!(out, "\n## Tiers\n");
    for tier in tier_names {
        let _ = writeln!(out, "- {tier}");
    }
}

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

    /// All available metric views with optional descriptions.
    pub metric_views: Vec<AboutGraphMetricViewInfo>,

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
pub struct AboutGraphMetricViewInfo {
    pub view: MetricView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    let metric_views = collect_metric_view_infos(ag);
    let graph_settings = ag.data.graph_settings.clone();
    let properties = ag.data.properties.clone();
    let text = render_markdown(handle, description.as_deref(), &stats, &metric_views);

    Ok(AboutGraphOutput {
        description,
        stats,
        metric_views,
        graph_settings,
        properties,
        text,
    })
}

fn extract_description(ag: &ArrayGraph) -> Option<String> {
    ag.data
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.description.clone())
}

fn collect_metric_view_infos(ag: &ArrayGraph) -> Vec<AboutGraphMetricViewInfo> {
    let metric_settings = ag
        .data
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.ui_settings.as_ref())
        .and_then(|ui| ui.columns.as_ref())
        .and_then(|cols| cols.metric_settings.as_ref());

    ag.enabled_metric_views()
        .into_iter()
        .map(|view| {
            let key = view.to_string();
            let description = metric_settings
                .and_then(|ms| ms.get(key.as_str()))
                .and_then(|s| s.description.clone());
            AboutGraphMetricViewInfo { view, description }
        })
        .collect()
}

// ── Markdown rendering ──────────────────────────────────────

fn render_markdown(
    handle: &str,
    description: Option<&str>,
    stats: &unigraph_core::ArrayGraphStats,
    metric_views: &[AboutGraphMetricViewInfo],
) -> String {
    let mut out = String::with_capacity(512);

    write_heading(&mut out, handle);
    write_description(&mut out, description);
    write_stats(&mut out, stats);
    write_metric_views(&mut out, metric_views);
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

fn write_metric_views(out: &mut String, metric_views: &[AboutGraphMetricViewInfo]) {
    if metric_views.is_empty() {
        return;
    }

    let _ = writeln!(out, "\n## Metric Views\n");
    for mv in metric_views {
        if let Some(desc) = &mv.description {
            let _ = writeln!(out, "- `{}` — {}", mv.view, desc);
        } else {
            let _ = writeln!(out, "- `{}`", mv.view);
        }
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

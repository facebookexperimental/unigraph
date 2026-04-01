// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use unigraph_core::ArrayGraph;
use unigraph_rpc::RpcExec;

use crate::AboutGraphInput;
use crate::AboutGraphMetricInfo;
use crate::AboutGraphOutput;
use crate::GraphHandle;
use crate::Unigraph;

impl RpcExec<Unigraph> for AboutGraphInput {
    type Output = AboutGraphOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<AboutGraphOutput> {
        let handle: GraphHandle = self.handle.parse()?;
        let ttl = Duration::from_secs(5 * 60);
        let ag = handle.resolve(ctx, task, ttl).await?;
        let handle_str = self.handle;
        tokio::task::spawn_blocking(move || build_about(&ag, &handle_str)).await?
    }
}

// ── Build output ────────────────────────────────────────────

fn build_about(ag: &Arc<ArrayGraph>, handle: &str) -> Result<AboutGraphOutput> {
    let stats = ag.stats();
    let description = extract_description(ag);
    let metrics = collect_metrics_info(ag);
    let text = render_markdown(handle, description.as_deref(), &stats, &metrics);

    Ok(AboutGraphOutput {
        description,
        stats,
        metrics,
        text,
    })
}

fn extract_description(ag: &ArrayGraph) -> Option<String> {
    ag.graph_settings
        .as_ref()
        .and_then(|gs| gs.description.clone())
}

fn collect_metrics_info(ag: &ArrayGraph) -> Vec<AboutGraphMetricInfo> {
    let metric_settings = ag
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.ui_settings.as_ref())
        .and_then(|ui| ui.columns.as_ref())
        .and_then(|cols| cols.metric_settings.as_ref());

    ag.metrics
        .keys()
        .map(|name| {
            let description = metric_settings
                .and_then(|ms| ms.get(name.as_str()))
                .and_then(|s| s.description.clone());
            AboutGraphMetricInfo {
                name: name.clone(),
                description,
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
) -> String {
    let mut out = String::with_capacity(512);

    write_heading(&mut out, handle);
    write_description(&mut out, description);
    write_stats(&mut out, stats);
    write_metrics(&mut out, metrics);
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
    for m in metrics {
        if let Some(desc) = &m.description {
            let _ = writeln!(out, "- `{}` — {}", m.name, desc);
        } else {
            let _ = writeln!(out, "- `{}`", m.name);
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

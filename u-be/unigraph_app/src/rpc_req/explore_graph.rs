// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use unigraph_core::ArrayGraph;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::NodeIDX;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::GraphKeyOrTimelineID;

use crate::ExploreGraphArrow;
use crate::ExploreGraphInput;
use crate::ExploreGraphOutput;
use crate::NodeMetric;
use crate::Unigraph;

impl RpcExec<Unigraph> for ExploreGraphInput {
    type Output = ExploreGraphOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<ExploreGraphOutput> {
        let gqc = resolve_gqc(ctx, &self, task).await?;
        let ag_ser = fetch_graph(ctx, &gqc, task).await?;
        let input = self;
        tokio::task::spawn_blocking(move || explore_node(ag_ser, &gqc, &input)).await?
    }
}

// ── Async helpers (graph fetching) ──────────────────────────────

async fn resolve_gqc(
    ctx: &Unigraph,
    input: &ExploreGraphInput,
    task: &ll::Task,
) -> Result<GraphQueryConfig> {
    match (&input.graph_query_config, &input.graph_query_config_key) {
        (Some(gqc), _) => Ok(gqc.clone()),
        (_, Some(key)) => ctx.db.configs.fetch_graph_query_config(key, task).await,
        (None, None) => bail!("either graph_query_config or graph_query_config_key must be set"),
    }
}

async fn fetch_graph(
    ctx: &Unigraph,
    gqc: &GraphQueryConfig,
    task: &ll::Task,
) -> Result<ArrayGraphSerializable> {
    let handle = gqc
        .handle
        .as_deref()
        .context("graph_query_config.handle is required")?;
    let parsed: GraphKeyOrTimelineID = handle.parse()?;
    let (_key, mut ag) = match parsed {
        GraphKeyOrTimelineID::GraphKey(key) => {
            let graph = ctx.db.graph.fetch(&key, task).await?;
            (key, graph)
        }
        GraphKeyOrTimelineID::TimelineID(tid) => ctx.db.graph.fetch_latest(&tid, task).await?,
    };

    if !gqc.roots.is_empty() {
        let root_idxs: Vec<_> = gqc
            .roots
            .iter()
            .filter_map(|name| ag.node_names_ordered.name_to_idx_log(name.as_str()))
            .collect();
        ag = ag
            .into_array_graph()
            .get_reachable_subgraph_unconfigured(&root_idxs)?;
    }

    Ok(ag)
}

// ── Sync core logic (runs in spawn_blocking) ────────────────────

fn explore_node(
    ag_ser: ArrayGraphSerializable,
    gqc: &GraphQueryConfig,
    input: &ExploreGraphInput,
) -> Result<ExploreGraphOutput> {
    let mut ag = ag_ser.into_array_graph();
    apply_traversal(&mut ag, gqc)?;

    let metric_names = collect_metric_names(&ag);
    let tier_names = collect_tier_names(&ag);
    let entry_points = ag.determine_entrypoints();

    let (parent_idx, arrow_data) =
        resolve_arrows(&ag, &input.node, input.graph_structure, &entry_points)?;
    let total_arrows_count = arrow_data.len();

    let sort_order = input.sort_order.unwrap_or(SortOrder::Desc);
    let sorted = sort_arrows(
        &ag,
        arrow_data,
        input.sort_by.as_ref(),
        sort_order,
        input.graph_structure,
    )?;

    let offset = input.offset.unwrap_or(0);
    let limit = input.limit.unwrap_or(50);
    let page = paginate(&sorted, offset, limit);

    let arrows = build_explore_arrows(&ag, page, &input.metrics, input.graph_structure)?;

    let node = parent_idx
        .map(|idx| build_explore_arrow_for_node(&ag, idx, &input.metrics, input.graph_structure))
        .transpose()?;

    let include_ascii = input.include_ascii.unwrap_or(false);
    let ascii = if include_ascii {
        Some(render_ascii(&node, &arrows, total_arrows_count, offset))
    } else {
        None
    };

    Ok(ExploreGraphOutput {
        node,
        arrows,
        metric_names,
        tier_names,
        total_arrows_count,
        ascii,
    })
}

fn apply_traversal(ag: &mut ArrayGraph, gqc: &GraphQueryConfig) -> Result<()> {
    let tvc = gqc
        .traversal_config
        .as_ref()
        .or(ag.state.traversal_config.as_ref());
    if let Some(tvc) = tvc {
        ag.apply_traversal_config(tvc.clone())?;
    }
    Ok(())
}

fn collect_metric_names(ag: &ArrayGraph) -> Vec<String> {
    ag.metrics.keys().cloned().collect()
}

fn collect_tier_names(ag: &ArrayGraph) -> Vec<String> {
    ag.state
        .traversal_config
        .as_ref()
        .and_then(|tc| tc.tiered_traversal.as_ref())
        .map(|tt| match tt {
            unigraph_core::TieredTraversalConfig::AscendingTiers(at) => {
                at.tiers.iter().map(|t| t.name.clone()).collect()
            }
        })
        .unwrap_or_default()
}

// ── Arrow resolution ────────────────────────────────────────────

/// Lightweight arrow data before full metric computation.
struct ArrowData {
    node_idx: NodeIDX,
    tag: Option<String>,
    dynamic: Option<unigraph_core::DynamicEdgeInfo>,
}

fn resolve_arrows(
    ag: &ArrayGraph,
    node_name: &Option<String>,
    graph_structure: GraphStructure,
    entry_points: &[NodeIDX],
) -> Result<(Option<NodeIDX>, Vec<ArrowData>)> {
    match node_name {
        None => {
            let arrows = entry_points
                .iter()
                .filter(|&&idx| !ag.is_node_unreachable(idx))
                .map(|&idx| ArrowData {
                    node_idx: idx,
                    tag: None,
                    dynamic: None,
                })
                .collect();
            Ok((None, arrows))
        }
        Some(name) => {
            let node_idx = ag
                .nodes
                .name_to_idx_log(name)
                .with_context(|| format!("node '{name}' not found in graph"))?;
            let arrows = ag
                .get_arrows(node_idx, graph_structure)?
                .into_iter()
                .filter(|a| !a.excluded)
                .map(|a| ArrowData {
                    node_idx: a.points_to,
                    tag: a.tag,
                    dynamic: a.dynamic,
                })
                .collect();
            Ok((Some(node_idx), arrows))
        }
    }
}

// ── Sorting ─────────────────────────────────────────────────────

fn sort_arrows(
    ag: &ArrayGraph,
    mut arrows: Vec<ArrowData>,
    sort_by: Option<&NodeMetric>,
    sort_order: SortOrder,
    graph_structure: GraphStructure,
) -> Result<Vec<ArrowData>> {
    let sort_by = match sort_by {
        Some(metric) => metric,
        None => {
            // No sort requested — return in natural order (entry points order, or edge order)
            return Ok(arrows);
        }
    };

    // Compute the sort metric value for each arrow
    let mut valued: Vec<(usize, f32)> = arrows
        .iter()
        .enumerate()
        .map(|(i, ad)| {
            let val = compute_metric(ag, ad.node_idx, sort_by, graph_structure)?;
            Ok((i, val))
        })
        .collect::<Result<Vec<_>>>()?;

    valued.sort_by(|a, b| match sort_order {
        SortOrder::Asc => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal),
        SortOrder::Desc => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal),
    });

    let sorted = valued
        .into_iter()
        .map(|(i, _)| std::mem::replace(&mut arrows[i], placeholder_arrow()))
        .collect();

    Ok(sorted)
}

fn placeholder_arrow() -> ArrowData {
    ArrowData {
        node_idx: NodeIDX::from(0u32),
        tag: None,
        dynamic: None,
    }
}

// ── Pagination ──────────────────────────────────────────────────

fn paginate(arrows: &[ArrowData], offset: usize, limit: usize) -> &[ArrowData] {
    let start = offset.min(arrows.len());
    let end = (start + limit).min(arrows.len());
    &arrows[start..end]
}

// ── Metric computation ──────────────────────────────────────────

/// Compute a single metric value for a node.
fn compute_metric(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metric: &NodeMetric,
    graph_structure: GraphStructure,
) -> Result<f32> {
    match metric {
        NodeMetric::Metric { name } => {
            Ok(ag.metrics.get(name.as_str()).map_or(0.0, |v| v[node_idx]))
        }
        NodeMetric::MetricTransitive { name } => {
            ag.get_transitive_metric_value(node_idx, name, false)
        }
        NodeMetric::MetricDominated { name } => {
            ag.get_transitive_metric_value(node_idx, name, true)
        }
        NodeMetric::MetricTiered { name, tier } => {
            let tiered = ag.get_transitive_tiered_metric_values(node_idx, name, false)?;
            Ok(*tiered.get(tier.as_str()).unwrap_or(&0.0))
        }
        NodeMetric::ParentsCount {} => Ok(ag.parents_len_configured(node_idx) as f32),
        NodeMetric::ChildrenCount {} => {
            let offset_graph = match graph_structure {
                GraphStructure::Forward => &ag.edges_forward,
                GraphStructure::Reverse => &ag.derived_state.edges_reverse,
                GraphStructure::Dominator => ag.edges_dom(),
            };
            Ok(offset_graph.edges_configured(node_idx).count() as f32)
        }
        NodeMetric::CountTransitive {} => Ok(ag.transitive_count_configured(node_idx) as f32),
        NodeMetric::CountDominated {} => {
            Ok(ag.transitive_count_configured_dominated(node_idx) as f32)
        }
    }
}

/// Build the flat metrics map for a node from the requested metric list.
fn build_metrics_map(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metrics: &[NodeMetric],
    graph_structure: GraphStructure,
) -> Result<BTreeMap<String, f32>> {
    let mut map = BTreeMap::new();
    for metric in metrics {
        let value = compute_metric(ag, node_idx, metric, graph_structure)?;
        if value != 0.0 {
            map.insert(metric.key(), value);
        }
    }
    Ok(map)
}

// ── Building output arrows ──────────────────────────────────────

fn build_explore_arrows(
    ag: &ArrayGraph,
    arrow_data: &[ArrowData],
    metrics: &[NodeMetric],
    graph_structure: GraphStructure,
) -> Result<Vec<ExploreGraphArrow>> {
    arrow_data
        .iter()
        .map(|ad| {
            let metrics = build_metrics_map(ag, ad.node_idx, metrics, graph_structure)?;
            Ok(ExploreGraphArrow {
                name: ag.idx_to_name(ad.node_idx).to_string(),
                metrics,
                tag: ad.tag.clone(),
                dynamic: ad.dynamic.clone(),
            })
        })
        .collect()
}

fn build_explore_arrow_for_node(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metrics: &[NodeMetric],
    graph_structure: GraphStructure,
) -> Result<ExploreGraphArrow> {
    let metrics = build_metrics_map(ag, node_idx, metrics, graph_structure)?;
    Ok(ExploreGraphArrow {
        name: ag.idx_to_name(node_idx).to_string(),
        metrics,
        tag: None,
        dynamic: None,
    })
}

// ── ASCII table rendering ───────────────────────────────────────

fn render_ascii(
    node: &Option<ExploreGraphArrow>,
    arrows: &[ExploreGraphArrow],
    total_count: usize,
    offset: usize,
) -> String {
    let metric_cols = collect_metric_columns(node, arrows);
    let has_tags = has_any_tags(node, arrows);
    let has_children = node.is_some();
    let widths = compute_column_widths(&metric_cols, has_tags, has_children, node, arrows);

    let mut out = String::with_capacity(256);
    write_header(&mut out, &metric_cols, has_tags, &widths);
    write_separator(&mut out, '=', &widths);

    if let Some(n) = node {
        write_row(&mut out, n, &metric_cols, has_tags, &widths, 0);
    }

    for arrow in arrows {
        let indent = if has_children { 2 } else { 0 };
        write_row(&mut out, arrow, &metric_cols, has_tags, &widths, indent);
    }

    write_footer(&mut out, arrows.len(), total_count, offset);
    out
}

fn collect_metric_columns(
    node: &Option<ExploreGraphArrow>,
    arrows: &[ExploreGraphArrow],
) -> Vec<String> {
    let mut cols = BTreeMap::<String, ()>::new();
    if let Some(n) = node {
        for k in n.metrics.keys() {
            cols.insert(k.clone(), ());
        }
    }
    for a in arrows {
        for k in a.metrics.keys() {
            cols.insert(k.clone(), ());
        }
    }
    cols.into_keys().collect()
}

fn has_any_tags(node: &Option<ExploreGraphArrow>, arrows: &[ExploreGraphArrow]) -> bool {
    node.as_ref().is_some_and(|n| n.tag.is_some()) || arrows.iter().any(|a| a.tag.is_some())
}

/// Column widths: [name, metric0, metric1, ..., tag?]
fn compute_column_widths(
    metric_cols: &[String],
    has_tags: bool,
    has_children: bool,
    node: &Option<ExploreGraphArrow>,
    arrows: &[ExploreGraphArrow],
) -> Vec<usize> {
    let num_cols = 1 + metric_cols.len() + usize::from(has_tags);
    let mut widths = Vec::with_capacity(num_cols);

    // name column — children are indented by 2 spaces
    let child_indent = if has_children { 2 } else { 0 };
    let mut name_w = 4; // "name"
    if let Some(n) = node {
        name_w = name_w.max(n.name.len());
    }
    for a in arrows {
        name_w = name_w.max(child_indent + a.name.len());
    }
    widths.push(name_w);

    // metric columns
    for col in metric_cols {
        let mut w = col.len();
        if let Some(n) = node
            && let Some(v) = n.metrics.get(col)
        {
            w = w.max(format_metric(*v).len());
        }
        for a in arrows {
            if let Some(v) = a.metrics.get(col) {
                w = w.max(format_metric(*v).len());
            }
        }
        widths.push(w);
    }

    // tag column
    if has_tags {
        let mut w = 3; // "tag"
        if let Some(n) = node
            && let Some(t) = &n.tag
        {
            w = w.max(t.len());
        }
        for a in arrows {
            if let Some(t) = &a.tag {
                w = w.max(t.len());
            }
        }
        widths.push(w);
    }

    widths
}

fn write_header(out: &mut String, metric_cols: &[String], has_tags: bool, widths: &[usize]) {
    let start = out.len();
    write_cell(out, "name", widths[0], true);
    for (i, col) in metric_cols.iter().enumerate() {
        let _ = write!(out, " | ");
        write_cell(out, col, widths[1 + i], false);
    }
    if has_tags {
        let _ = write!(out, " | ");
        write_cell(out, "tag", widths[widths.len() - 1], true);
    }
    trim_trailing_spaces(out, start);
    out.push('\n');
}

fn write_separator(out: &mut String, ch: char, widths: &[usize]) {
    for (i, &w) in widths.iter().enumerate() {
        if i > 0 {
            out.push(ch);
            out.push('+');
            out.push(ch);
        }
        for _ in 0..w {
            out.push(ch);
        }
    }
    out.push('\n');
}

fn write_row(
    out: &mut String,
    arrow: &ExploreGraphArrow,
    metric_cols: &[String],
    has_tags: bool,
    widths: &[usize],
    indent: usize,
) {
    let start = out.len();
    let name = if indent > 0 {
        format!("{:indent$}{}", "", arrow.name)
    } else {
        arrow.name.clone()
    };
    write_cell(out, &name, widths[0], true);
    for (i, col) in metric_cols.iter().enumerate() {
        let _ = write!(out, " | ");
        let val = arrow
            .metrics
            .get(col)
            .map(|v| format_metric(*v))
            .unwrap_or_else(|| "0".to_string());
        write_cell(out, &val, widths[1 + i], false);
    }
    if has_tags {
        let _ = write!(out, " | ");
        let tag = arrow.tag.as_deref().unwrap_or("");
        write_cell(out, tag, widths[widths.len() - 1], true);
    }
    trim_trailing_spaces(out, start);
    out.push('\n');
}

fn write_footer(out: &mut String, shown: usize, total: usize, offset: usize) {
    if total > shown {
        let _ = write!(out, "\n(showing {shown} of {total} rows, offset {offset})");
    }
}

fn write_cell(out: &mut String, text: &str, width: usize, left_align: bool) {
    if left_align {
        let _ = write!(out, "{text:<width$}");
    } else {
        let _ = write!(out, "{text:>width$}");
    }
}

fn trim_trailing_spaces(out: &mut String, start: usize) {
    let trimmed = out[start..].trim_end_matches(' ').len();
    out.truncate(start + trimmed);
}

fn format_metric(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

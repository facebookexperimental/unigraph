// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::DynamicEdgeInfo;
/// Re-export MetricView for use in ExploreGraphInput.
pub use unigraph_core::MetricView;
use unigraph_core::NodeIDX;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;
use unigraph_rpc::RpcExec;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

/// What to explore.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub enum ExploreGraphTarget {
    /// Auto-detected entry points (nodes with no parents).
    EntryPoints {},
    /// Drill into a specific node's children.
    Node { name: String },
    /// Flat list of all reachable nodes.
    AllNodes {},
}

impl Default for ExploreGraphTarget {
    fn default() -> Self {
        Self::EntryPoints {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreGraphInput {
    /// The query config: which graph, optional roots, optional traversal.
    pub query: GraphQueryConfig,

    /// What to explore: entry points, a specific node, or all nodes.
    #[serde(default)]
    pub target: ExploreGraphTarget,

    /// Which edge structure to follow.
    #[serde(default)]
    pub graph_structure: GraphStructure,

    /// Which metrics to compute for each arrow.
    /// - `None` (default): return all available metric views.
    /// - `Some([])`: return no metrics.
    /// - `Some([...])`: return exactly the listed metrics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<MetricView>>,

    /// Metric to sort arrows by. Computed for all children (even beyond limit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<MetricView>,

    /// Sort order. Defaults to Desc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<SortOrder>,

    /// Skip first N results (for pagination).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,

    /// Maximum number of arrows to return. Defaults to 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// When true, populate the `ascii` field in the response with a human-readable
    /// ASCII table of the results (optimized for agent / LLM consumption).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ascii: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreGraphOutput {
    /// The node being explored, with its own metrics. None when showing entry points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<ExploreGraphArrow>,
    /// Arrows to children (or to entry points when node is None).
    pub arrows: Vec<ExploreGraphArrow>,
    /// Available metric names in this graph.
    pub metric_names: Vec<String>,
    /// Tier names if tiered traversal is configured.
    pub tier_names: Vec<String>,
    /// Total number of arrows before offset/limit.
    pub total_arrows_count: usize,
    /// Human-readable ASCII table of the results. Only populated when
    /// `include_ascii` is set to true in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreGraphArrow {
    /// Node name.
    pub name: String,
    /// Flat metrics map. Keys follow naming conventions:
    /// - "{metric}" — self value
    /// - "{metric}_transitive" — transitive sum
    /// - "{metric}_dominated" — dominated sum
    /// - "{metric}_{tier}" — tiered transitive (if tiers configured)
    /// - "parents_count" — number of configured parents
    /// - "children_count" — number of children in current graph structure
    pub metrics: BTreeMap<String, f64>,
    /// Edge tag (e.g. "lazy"), if this is a tagged edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Dynamic edge info, if this is a dynamic edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<DynamicEdgeInfo>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for ExploreGraphInput {
    type Output = ExploreGraphOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<ExploreGraphOutput> {
        let ttl = Duration::from_mins(5);
        let ag = ctx.graph_cache.get_explored(&self.query, task, ttl).await?;
        let input = self;
        tokio::task::spawn_blocking(move || explore_node(ag, &input)).await?
    }
}

// ── Sync core logic (runs in spawn_blocking) ────────────────────

fn explore_node(ag: Arc<ArrayGraph>, input: &ExploreGraphInput) -> Result<ExploreGraphOutput> {
    let metric_names = collect_metric_names(&ag);
    let tier_names = collect_tier_names(&ag);
    let metrics = resolve_metrics(&ag, &input.metrics, input.graph_structure)?;

    let (parent_idx, arrow_data) = resolve_arrows(&ag, &input.target, input.graph_structure)?;
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

    let arrows = build_explore_arrows(&ag, page, &metrics, input.graph_structure)?;

    let node = parent_idx
        .map(|idx| build_explore_arrow_for_node(&ag, idx, &metrics, input.graph_structure))
        .transpose()?;

    let include_ascii = input.include_ascii.unwrap_or(false);
    let sort_by_key = input.sort_by.as_ref().map(|m| m.to_string());
    let metrics_config = ag
        .graph_settings()
        .and_then(|gs| gs.metrics_config.as_ref());
    let ascii = if include_ascii {
        Some(render_ascii(
            &input.target,
            input.graph_structure,
            node.as_ref(),
            &arrows,
            total_arrows_count,
            offset,
            sort_by_key.as_deref(),
            sort_order,
            metrics_config,
            &tier_names,
        ))
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

fn collect_metric_names(ag: &ArrayGraph) -> Vec<String> {
    ag.data.node_metadata.metrics.keys().cloned().collect()
}

fn collect_tier_names(ag: &ArrayGraph) -> Vec<String> {
    ag.runtime
        .state
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

/// Resolve the metrics list:
///   `None`        → visible views for this graph structure
///   `Some(list)`  → exactly that list, validated against available views
fn resolve_metrics(
    ag: &ArrayGraph,
    metrics: &Option<Vec<MetricView>>,
    structure: GraphStructure,
) -> Result<Vec<MetricView>> {
    match metrics {
        None => Ok(ag.visible_metric_views(structure)),
        Some(list) => {
            let available: std::collections::BTreeSet<String> = ag
                .available_metric_views()
                .iter()
                .map(|v| v.to_string())
                .collect();
            let invalid: Vec<String> = list
                .iter()
                .filter(|v| !available.contains(&v.to_string()))
                .map(|v| v.to_string())
                .collect();
            if !invalid.is_empty() {
                let mut available_sorted: Vec<&String> = available.iter().collect();
                available_sorted.sort();
                bail!(
                    "Unknown metric view(s): {}. Available views:\n{}",
                    invalid.join(", "),
                    available_sorted
                        .iter()
                        .map(|v| format!("  - {v}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
            Ok(list.clone())
        }
    }
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
    target: &ExploreGraphTarget,
    graph_structure: GraphStructure,
) -> Result<(Option<NodeIDX>, Vec<ArrowData>)> {
    match target {
        ExploreGraphTarget::EntryPoints {} => {
            let arrows = ag
                .determine_entrypoints()
                .into_iter()
                .filter(|&idx| !ag.is_node_unreachable(idx))
                .map(|idx| ArrowData {
                    node_idx: idx,
                    tag: None,
                    dynamic: None,
                })
                .collect();
            Ok((None, arrows))
        }
        ExploreGraphTarget::AllNodes {} => {
            let arrows = ag
                .all_reachable_node_idxs()
                .into_iter()
                .map(|idx| ArrowData {
                    node_idx: idx,
                    tag: None,
                    dynamic: None,
                })
                .collect();
            Ok((None, arrows))
        }
        ExploreGraphTarget::Node { name } => {
            let node_idx = ag
                .data
                .node_names_ordered
                .name_to_idx_log(name)
                .with_context(|| format!("node '{name}' not found in graph"))?;
            let mut arrows: Vec<ArrowData> = ag
                .get_arrows(node_idx, graph_structure)?
                .into_iter()
                .filter(|a| !a.excluded)
                .map(|a| ArrowData {
                    node_idx: a.points_to,
                    tag: a.tag,
                    dynamic: a.dynamic,
                })
                .collect();

            // Reverse edges are computed lazily and may arrive in
            // non-deterministic order. Sort by node name so the natural
            // (unsorted) output is stable across runs.
            if graph_structure == GraphStructure::Reverse {
                arrows.sort_by(|a, b| ag.idx_to_name(a.node_idx).cmp(ag.idx_to_name(b.node_idx)));
            }

            Ok((Some(node_idx), arrows))
        }
    }
}

// ── Sorting ─────────────────────────────────────────────────────

fn sort_arrows(
    ag: &ArrayGraph,
    mut arrows: Vec<ArrowData>,
    sort_by: Option<&MetricView>,
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
    let mut valued: Vec<(usize, f64)> = arrows
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
    metric: &MetricView,
    _graph_structure: GraphStructure,
) -> Result<f64> {
    match metric {
        MetricView::Metric { name } => Ok(ag
            .data
            .node_metadata
            .metrics
            .get(name.as_str())
            .map_or(0.0, |v| v[node_idx])),
        MetricView::Transitive { name } => ag.get_transitive_metric_value(node_idx, name, false),
        MetricView::Dominated { name } => ag.get_transitive_metric_value(node_idx, name, true),
        MetricView::Tiered { name, tier_name } => {
            let tiered = ag.get_transitive_tiered_metric_values(node_idx, name, false)?;
            Ok(*tiered.get(tier_name.as_str()).unwrap_or(&0.0))
        }
        MetricView::TieredDominated { name, tier_name } => {
            let tiered = ag.get_transitive_tiered_metric_values(node_idx, name, true)?;
            Ok(*tiered.get(tier_name.as_str()).unwrap_or(&0.0))
        }
        MetricView::ParentsCount {} => Ok(ag.parents_len_configured(node_idx) as f64),
        MetricView::CountTransitive {} => Ok(ag.transitive_count_configured(node_idx) as f64),
        MetricView::CountDominated {} => {
            Ok(ag.transitive_count_configured_dominated(node_idx) as f64)
        }
        MetricView::TierIndex {} => Ok(ag.node_tier_idx(node_idx).unwrap_or(0) as f64),
    }
}

/// Build the flat metrics map for a node from the requested metric list.
fn build_metrics_map(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metrics: &[MetricView],
    graph_structure: GraphStructure,
) -> Result<BTreeMap<String, f64>> {
    let mut map = BTreeMap::new();
    for metric in metrics {
        let value = compute_metric(ag, node_idx, metric, graph_structure)?;
        map.insert(metric.to_string(), value);
    }
    Ok(map)
}

// ── Building output arrows ──────────────────────────────────────

fn build_explore_arrows(
    ag: &ArrayGraph,
    arrow_data: &[ArrowData],
    metrics: &[MetricView],
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
    metrics: &[MetricView],
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

/// Display width of a sort arrow suffix (" ▼" or " ▲").
const SORT_ARROW_DISPLAY_LEN: usize = 2;

fn sort_arrow(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Desc => " ▼",
        SortOrder::Asc => " ▲",
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "ASCII rendering helper with many formatting options"
)]
fn render_ascii(
    target: &ExploreGraphTarget,
    graph_structure: GraphStructure,
    node: Option<&ExploreGraphArrow>,
    arrows: &[ExploreGraphArrow],
    total_count: usize,
    offset: usize,
    sort_by_key: Option<&str>,
    sort_order: SortOrder,
    metrics_config: Option<&unigraph_core::graph_settings::MetricsConfig>,
    tier_names: &[String],
) -> String {
    let all_arrows = node.into_iter().chain(arrows.iter()).collect::<Vec<_>>();
    let metric_cols = collect_metric_columns_from(&all_arrows);
    let has_tags = all_arrows.iter().any(|a| a.tag.is_some());
    let has_dynamic = all_arrows.iter().any(|a| a.dynamic.is_some());

    let formats = build_format_map(&metric_cols, metrics_config);

    let widths = compute_column_widths(
        &metric_cols,
        has_tags,
        has_dynamic,
        &all_arrows,
        sort_by_key,
        &formats,
        tier_names,
    );

    let mut out = String::with_capacity(256);
    write_summary(&mut out, target, graph_structure);
    write_header(
        &mut out,
        &metric_cols,
        has_tags,
        has_dynamic,
        &widths,
        sort_by_key,
        sort_order,
    );
    write_separator(&mut out, '=', &widths);

    if let Some(parent) = node {
        write_row(
            &mut out,
            parent,
            &metric_cols,
            has_tags,
            has_dynamic,
            &widths,
            &formats,
            tier_names,
        );
        write_separator(&mut out, '-', &widths);
    }

    for arrow in arrows {
        write_row(
            &mut out,
            arrow,
            &metric_cols,
            has_tags,
            has_dynamic,
            &widths,
            &formats,
            tier_names,
        );
    }

    write_footer(&mut out, arrows.len(), total_count, offset);
    out
}

fn write_summary(out: &mut String, target: &ExploreGraphTarget, graph_structure: GraphStructure) {
    match target {
        ExploreGraphTarget::EntryPoints {} => {
            out.push_str("Entry points\n\n");
        }
        ExploreGraphTarget::AllNodes {} => {
            out.push_str("All reachable nodes\n\n");
        }
        ExploreGraphTarget::Node { name } => {
            let structure = match graph_structure {
                GraphStructure::Forward => "forward",
                GraphStructure::Reverse => "reverse",
                GraphStructure::Dominator => "dominator",
            };
            let _ = writeln!(out, "Edges: {structure}");
            let _ = writeln!(out, "Edges of: {name}");
            out.push('\n');
        }
    }
}

fn collect_metric_columns_from(arrows: &[&ExploreGraphArrow]) -> Vec<String> {
    let mut cols = BTreeMap::<String, ()>::new();
    for a in arrows {
        for k in a.metrics.keys() {
            cols.insert(k.clone(), ());
        }
    }
    cols.into_keys().collect()
}

fn format_dynamic(d: &DynamicEdgeInfo) -> String {
    format!("{}:{}/{}", d.type_key, d.edge_name, d.branch)
}

/// Column widths: [node_name, metric0, metric1, ..., tag?, edge?]
fn compute_column_widths(
    metric_cols: &[String],
    has_tags: bool,
    has_dynamic: bool,
    arrows: &[&ExploreGraphArrow],
    sort_by_key: Option<&str>,
    formats: &BTreeMap<String, unigraph_core::graph_settings::MetricFormat>,
    tier_names: &[String],
) -> Vec<usize> {
    let num_cols = 1 + metric_cols.len() + usize::from(has_tags) + usize::from(has_dynamic);
    let mut widths = Vec::with_capacity(num_cols);

    // node_name column
    let mut name_w = 9; // "node_name"
    for a in arrows {
        name_w = name_w.max(a.name.len());
    }
    widths.push(name_w);

    // metric columns
    for col in metric_cols {
        let header_w = if sort_by_key == Some(col.as_str()) {
            col.len() + SORT_ARROW_DISPLAY_LEN
        } else {
            col.len()
        };
        let mut w = header_w;
        for a in arrows {
            if let Some(v) = a.metrics.get(col) {
                w = w.max(format_cell_value(*v, col, formats, tier_names).len());
            }
        }
        widths.push(w);
    }

    // tag column
    if has_tags {
        let mut w = 3; // "tag"
        for a in arrows {
            if let Some(t) = &a.tag {
                w = w.max(t.len());
            }
        }
        widths.push(w);
    }

    // dynamic edge column
    if has_dynamic {
        let mut w = 7; // "dynamic"
        for a in arrows {
            if let Some(d) = &a.dynamic {
                w = w.max(format_dynamic(d).len());
            }
        }
        widths.push(w);
    }

    widths
}

fn write_header(
    out: &mut String,
    metric_cols: &[String],
    has_tags: bool,
    has_dynamic: bool,
    widths: &[usize],
    sort_by_key: Option<&str>,
    sort_order: SortOrder,
) {
    let start = out.len();
    write_cell(out, "node_name", widths[0], true);
    for (i, col) in metric_cols.iter().enumerate() {
        let _ = write!(out, " | ");
        if sort_by_key == Some(col.as_str()) {
            let text = format!("{col}{}", sort_arrow(sort_order));
            // ▼/▲ is 3 bytes but 1 display char — pad manually for correct alignment
            let display_len = col.len() + SORT_ARROW_DISPLAY_LEN;
            let pad = widths[1 + i].saturating_sub(display_len);
            for _ in 0..pad {
                out.push(' ');
            }
            out.push_str(&text);
        } else {
            write_cell(out, col, widths[1 + i], false);
        }
    }
    let mut extra_idx = 1 + metric_cols.len();
    if has_tags {
        let _ = write!(out, " | ");
        write_cell(out, "tag", widths[extra_idx], true);
        extra_idx += 1;
    }
    if has_dynamic {
        let _ = write!(out, " | ");
        write_cell(out, "dynamic", widths[extra_idx], true);
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

#[expect(
    clippy::too_many_arguments,
    reason = "ASCII rendering helper with many formatting options"
)]
fn write_row(
    out: &mut String,
    arrow: &ExploreGraphArrow,
    metric_cols: &[String],
    has_tags: bool,
    has_dynamic: bool,
    widths: &[usize],
    formats: &BTreeMap<String, unigraph_core::graph_settings::MetricFormat>,
    tier_names: &[String],
) {
    let start = out.len();
    write_cell(out, &arrow.name, widths[0], true);
    for (i, col) in metric_cols.iter().enumerate() {
        let _ = write!(out, " | ");
        let val = arrow
            .metrics
            .get(col)
            .map(|v| format_cell_value(*v, col, formats, tier_names))
            .unwrap_or_else(|| "0".to_string());
        write_cell(out, &val, widths[1 + i], false);
    }
    let mut extra_idx = 1 + metric_cols.len();
    if has_tags {
        let _ = write!(out, " | ");
        let tag = arrow.tag.as_deref().unwrap_or("");
        write_cell(out, tag, widths[extra_idx], true);
        extra_idx += 1;
    }
    if has_dynamic {
        let _ = write!(out, " | ");
        let edge = arrow
            .dynamic
            .as_ref()
            .map(format_dynamic)
            .unwrap_or_default();
        write_cell(out, &edge, widths[extra_idx], true);
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
    let pad = width.saturating_sub(text.len());
    if left_align {
        out.push_str(text);
        for _ in 0..pad {
            out.push(' ');
        }
    } else {
        for _ in 0..pad {
            out.push(' ');
        }
        out.push_str(text);
    }
}

fn trim_trailing_spaces(out: &mut String, start: usize) {
    let trimmed = out[start..].trim_end_matches(' ').len();
    out.truncate(start + trimmed);
}

fn format_cell_value(
    v: f64,
    col: &str,
    formats: &BTreeMap<String, unigraph_core::graph_settings::MetricFormat>,
    tier_names: &[String],
) -> String {
    if let Ok(unigraph_core::MetricView::TierIndex {}) = col.parse::<unigraph_core::MetricView>() {
        let idx = v as usize;
        return tier_names
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("T{idx}"));
    }
    match formats.get(col) {
        Some(f) => f.format_value(v),
        None => {
            if v == v.trunc() && v.abs() < 1e15 {
                format!("{}", v as i64)
            } else {
                format!("{v:.2}")
            }
        }
    }
}

fn build_format_map(
    metric_cols: &[String],
    metrics_config: Option<&unigraph_core::graph_settings::MetricsConfig>,
) -> BTreeMap<String, unigraph_core::graph_settings::MetricFormat> {
    let mut map: BTreeMap<String, unigraph_core::graph_settings::MetricFormat> = BTreeMap::new();
    let Some(config) = metrics_config else {
        return map;
    };
    for col in metric_cols {
        let view: unigraph_core::MetricView = match col.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(fmt) = config.format_for_view(&view) {
            map.insert(col.clone(), fmt.clone());
        }
    }
    map
}

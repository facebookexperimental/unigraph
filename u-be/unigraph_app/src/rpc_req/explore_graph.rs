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
use unigraph_core::NameMatchMode;
use unigraph_core::NodeIDX;
use unigraph_core::NodeSelection;
use unigraph_core::SelectOptions;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricsConfig;
use unigraph_core::graph_settings::SortOrder;
use unigraph_rpc::RpcExec;

use crate::Unigraph;
use crate::rpc_req::ascii_table::SORT_ARROW_DISPLAY_LEN;
use crate::rpc_req::ascii_table::build_format_map;
use crate::rpc_req::ascii_table::describe_selection;
use crate::rpc_req::ascii_table::format_cell_value;
use crate::rpc_req::ascii_table::sort_arrow;
use crate::rpc_req::ascii_table::trim_trailing_spaces;
use crate::rpc_req::ascii_table::write_cell;
use crate::rpc_req::ascii_table::write_separator;

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
    /// Flat list of the reachable nodes matching `selection` — by name,
    /// properties, or edge tags.
    Matching { selection: NodeSelection },
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

    /// When true, also return arrows the traversal did not follow, flagged via
    /// `excluded` / `unreachable`. Defaults to false, which drops them entirely.
    ///
    /// Only meaningful for the `Node` target — the other targets enumerate
    /// nodes, not edges, and already return reachable nodes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_excluded: Option<bool>,

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

    /// True when the traversal did not follow this edge. A property of the
    /// *edge* — the node it points to may still be reachable by another path.
    /// Only ever true when `include_excluded` was requested.
    ///
    /// `default` so a client built against this schema can still decode a
    /// response from a service that predates the field.
    #[serde(default)]
    pub excluded: bool,
    /// True when the node this arrow points to is not reachable from the entry
    /// points at all. A property of the *node*, so it can be true even for an
    /// edge that was followed (when the parent is itself unreachable).
    #[serde(default)]
    pub unreachable: bool,
    /// Why the traversal skipped this edge, e.g. "tag `lazy` is above max
    /// tier". Only the winning rule is recorded, not a full audit trail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusion_reason: Option<String>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for ExploreGraphInput {
    type Output = ExploreGraphOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<ExploreGraphOutput> {
        let ttl = Duration::from_mins(5);
        let ag = ctx.graph_cache.get_explored(&self.query, task, ttl).await?;
        let input = self;
        task.spawn("explore_graph", |task| async move {
            tokio::task::spawn_blocking(move || explore_node(ag, &input, &task))
                .await
                .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Sync core logic (runs in spawn_blocking) ────────────────────

fn explore_node(
    ag: Arc<ArrayGraph>,
    input: &ExploreGraphInput,
    task: &ll::Task,
) -> Result<ExploreGraphOutput> {
    let metric_names = collect_metric_names(&ag);
    let tier_names = collect_tier_names(&ag);
    let metrics = resolve_metrics(&ag, &input.metrics, input.graph_structure)?;

    let offset = input.offset.unwrap_or(0);
    let limit = input.limit.unwrap_or(50);

    let (parent_idx, arrow_data) = resolve_arrows(&ag, input, task)?;
    let total_arrows_count = arrow_data.len();

    let sort_order = input.sort_order.unwrap_or(SortOrder::Desc);
    let sorted = sort_arrows(&ag, arrow_data, input.sort_by.as_ref(), sort_order)?;

    let page = paginate(&sorted, offset, limit);

    let arrows = build_explore_arrows(&ag, page, &metrics)?;

    let node = parent_idx
        .map(|idx| build_explore_arrow_for_node(&ag, idx, &metrics))
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
    excluded: bool,
    unreachable: bool,
    exclusion_reason: Option<String>,
}

/// Turn a node index into a bare row — no edge led here, so no tag or branch,
/// and nothing to exclude. Every target that uses this enumerates reachable
/// nodes only, so `unreachable` is false by construction.
fn node_arrow(node_idx: NodeIDX) -> ArrowData {
    ArrowData {
        node_idx,
        tag: None,
        dynamic: None,
        excluded: false,
        unreachable: false,
        exclusion_reason: None,
    }
}

fn resolve_arrows(
    ag: &ArrayGraph,
    input: &ExploreGraphInput,
    task: &ll::Task,
) -> Result<(Option<NodeIDX>, Vec<ArrowData>)> {
    let graph_structure = input.graph_structure;
    match &input.target {
        ExploreGraphTarget::EntryPoints {} => {
            let arrows = ag
                .determine_entrypoints()
                .into_iter()
                .filter(|&idx| !ag.is_node_unreachable(idx))
                .map(node_arrow)
                .collect();
            Ok(flat(arrows))
        }
        ExploreGraphTarget::AllNodes {} => {
            let arrows = ag
                .all_reachable_node_idxs()
                .into_iter()
                .map(node_arrow)
                .collect();
            Ok(flat(arrows))
        }
        ExploreGraphTarget::Matching { selection } => {
            reject_top_k_name_mode(selection)?;
            let arrows = ag
                .select_nodes(selection, &ENUMERATE_ALL, task)?
                .into_iter()
                .map(node_arrow)
                .collect();
            Ok(flat(arrows))
        }
        ExploreGraphTarget::Node { name } => {
            let node_idx = ag
                .data
                .node_names_ordered
                .name_to_idx_log(name)
                .with_context(|| format!("node '{name}' not found in graph"))?;
            let include_excluded = input.include_excluded.unwrap_or(false);
            let mut arrows: Vec<ArrowData> = ag
                .get_arrows(node_idx, graph_structure)?
                .into_iter()
                .filter(|a| include_excluded || !a.excluded)
                .map(|a| ArrowData {
                    node_idx: a.points_to,
                    tag: a.tag,
                    dynamic: a.dynamic,
                    excluded: a.excluded,
                    unreachable: ag.is_node_unreachable(a.points_to),
                    exclusion_reason: a.message,
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

/// A flat list of rows — nothing was drilled into, so there's no parent.
fn flat(arrows: Vec<ArrowData>) -> (Option<NodeIDX>, Vec<ArrowData>) {
    (None, arrows)
}

/// Explore always enumerates the whole match set: `total_arrows_count` is a
/// real count, and sorting has to see every match, not just the current page.
pub(crate) const ENUMERATE_ALL: SelectOptions = SelectOptions {
    limit: None,
    reachable_only: true,
};

/// `Fuzzy` is top-K by construction, so it can only ever enumerate a capped
/// prefix — it cannot answer "how many nodes match", which is exactly what a
/// paginated table with a row count needs. Rejecting it here keeps the total
/// honest; `SearchNodes` is the surface that wants top-K.
pub(crate) fn reject_top_k_name_mode(selection: &NodeSelection) -> Result<()> {
    let is_fuzzy = selection
        .name_condition()
        .is_some_and(|name| name.mode == NameMatchMode::Fuzzy);
    if is_fuzzy {
        bail!(
            "the Fuzzy name mode is not supported when exploring: it is top-K, \
             so it cannot produce a total row count. Use Substring, Regex or \
             Exact here, or the SearchNodes RPC for typeahead-style matching."
        );
    }
    Ok(())
}

// ── Sorting ─────────────────────────────────────────────────────

fn sort_arrows(
    ag: &ArrayGraph,
    mut arrows: Vec<ArrowData>,
    sort_by: Option<&MetricView>,
    sort_order: SortOrder,
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
        .map(|(i, ad)| Ok((i, ag.metric_value(ad.node_idx, sort_by)?)))
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
        excluded: false,
        unreachable: false,
        exclusion_reason: None,
    }
}

// ── Pagination ──────────────────────────────────────────────────

fn paginate(arrows: &[ArrowData], offset: usize, limit: usize) -> &[ArrowData] {
    let start = offset.min(arrows.len());
    let end = (start + limit).min(arrows.len());
    &arrows[start..end]
}

// ── Metric computation ──────────────────────────────────────────

/// Build the flat metrics map for a node from the requested metric list.
fn build_metrics_map(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metrics: &[MetricView],
) -> Result<BTreeMap<String, f64>> {
    metrics
        .iter()
        .map(|metric| Ok((metric.to_string(), ag.metric_value(node_idx, metric)?)))
        .collect()
}

// ── Building output arrows ──────────────────────────────────────

fn build_explore_arrows(
    ag: &ArrayGraph,
    arrow_data: &[ArrowData],
    metrics: &[MetricView],
) -> Result<Vec<ExploreGraphArrow>> {
    arrow_data
        .iter()
        .map(|ad| {
            let metrics = build_metrics_map(ag, ad.node_idx, metrics)?;
            Ok(ExploreGraphArrow {
                name: ag.idx_to_name(ad.node_idx).to_string(),
                metrics,
                tag: ad.tag.clone(),
                dynamic: ad.dynamic.clone(),
                excluded: ad.excluded,
                unreachable: ad.unreachable,
                exclusion_reason: ad.exclusion_reason.clone(),
            })
        })
        .collect()
}

fn build_explore_arrow_for_node(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metrics: &[MetricView],
) -> Result<ExploreGraphArrow> {
    let metrics = build_metrics_map(ag, node_idx, metrics)?;
    Ok(ExploreGraphArrow {
        name: ag.idx_to_name(node_idx).to_string(),
        metrics,
        tag: None,
        dynamic: None,
        excluded: false,
        unreachable: ag.is_node_unreachable(node_idx),
        exclusion_reason: None,
    })
}

// ── ASCII table rendering ───────────────────────────────────────

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
    metrics_config: Option<&MetricsConfig>,
    tier_names: &[String],
) -> String {
    let all_arrows = node.into_iter().chain(arrows.iter()).collect::<Vec<_>>();
    let metric_cols = collect_metric_columns_from(&all_arrows);
    let has_tags = all_arrows.iter().any(|a| a.tag.is_some());
    let has_dynamic = all_arrows.iter().any(|a| a.dynamic.is_some());
    let has_status = all_arrows.iter().any(|a| arrow_status(a).is_some());

    let formats = build_format_map(&metric_cols, metrics_config);

    let widths = compute_column_widths(
        &metric_cols,
        has_tags,
        has_dynamic,
        has_status,
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
        has_status,
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
            has_status,
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
            has_status,
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
        ExploreGraphTarget::Matching { selection } => {
            let _ = writeln!(out, "Nodes matching {}", describe_selection(selection));
            out.push('\n');
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

/// Column header for the traversal-status column.
const STATUS_HEADER: &str = "status";
/// The edge was not followed, but the node it points to is still reachable
/// through some other path.
const STATUS_EXCLUDED: &str = "excluded";
/// The node is not reachable from the entry points at all.
const STATUS_UNREACHABLE: &str = "unreachable";

/// The one-word traversal status for a row, or `None` for an ordinary followed
/// edge to a reachable node.
///
/// `unreachable` wins over `excluded`: it is the strictly more severe fact, and
/// it is a property of the node rather than of this particular edge — the same
/// precedence the tree table UI uses.
fn arrow_status(arrow: &ExploreGraphArrow) -> Option<&'static str> {
    if arrow.unreachable {
        return Some(STATUS_UNREACHABLE);
    }
    if arrow.excluded {
        return Some(STATUS_EXCLUDED);
    }
    None
}

/// Column widths: [node_name, metric0, metric1, ..., tag?, edge?]
#[expect(
    clippy::too_many_arguments,
    reason = "ASCII rendering helper with many formatting options"
)]
fn compute_column_widths(
    metric_cols: &[String],
    has_tags: bool,
    has_dynamic: bool,
    has_status: bool,
    arrows: &[&ExploreGraphArrow],
    sort_by_key: Option<&str>,
    formats: &BTreeMap<String, MetricFormat>,
    tier_names: &[String],
) -> Vec<usize> {
    let num_cols = 1
        + metric_cols.len()
        + usize::from(has_tags)
        + usize::from(has_dynamic)
        + usize::from(has_status);
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

    // traversal status column
    if has_status {
        let mut w = STATUS_HEADER.len();
        for a in arrows {
            if let Some(status) = arrow_status(a) {
                w = w.max(status.len());
            }
        }
        widths.push(w);
    }

    widths
}

#[expect(
    clippy::too_many_arguments,
    reason = "ASCII rendering helper with many formatting options"
)]
fn write_header(
    out: &mut String,
    metric_cols: &[String],
    has_tags: bool,
    has_dynamic: bool,
    has_status: bool,
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
        extra_idx += 1;
    }
    if has_status {
        let _ = write!(out, " | ");
        write_cell(out, STATUS_HEADER, widths[extra_idx], true);
    }
    trim_trailing_spaces(out, start);
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
    has_status: bool,
    widths: &[usize],
    formats: &BTreeMap<String, MetricFormat>,
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
        extra_idx += 1;
    }
    if has_status {
        let _ = write!(out, " | ");
        write_cell(
            out,
            arrow_status(arrow).unwrap_or(""),
            widths[extra_idx],
            true,
        );
    }
    trim_trailing_spaces(out, start);
    out.push('\n');
}

fn write_footer(out: &mut String, shown: usize, total: usize, offset: usize) {
    if total > shown {
        let _ = write!(out, "\n(showing {shown} of {total} rows, offset {offset})");
    }
}

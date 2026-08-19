// Copyright (c) Meta Platforms, Inc. and affiliates.

//! ExploreDelta — the twin-graph counterpart of [`ExploreGraph`].
//!
//! Takes two graph handles instead of one and reports each node with `∆`
//! metric columns you can pick and sort by, plus a "changed nodes only" mode
//! that collapses untouched stretches of the graph and says how many nodes it
//! swallowed. This is the headless equivalent of the web UI's delta view.
//!
//! ```text
//! left  ──┐
//!         ├─→ GraphCache::get_twin ──→ Arc<TwinGraph> ──→ rows ──→ metrics ──→ ascii
//! right ──┘        (cached)
//! ```
//!
//! The merged `TwinGraph` is cached, so a burst of requests against the same
//! handle pair pays the merge cost once. See [`GraphCache::get_twin`].
//!
//! [`ExploreGraph`]: super::explore_graph
//! [`GraphCache::get_twin`]: crate::GraphCache::get_twin

mod ascii;
mod metrics;
mod rows;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::DynamicEdgeInfo;
use unigraph_core::MetricView;
use unigraph_core::NodeDiff;
use unigraph_core::NodeIDX;
use unigraph_core::TwinGraph;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;
use unigraph_rpc::RpcExec;

use super::ExploreGraphTarget;
use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreDeltaInput {
    /// The "before" graph.
    pub left: GraphQueryConfig,
    /// The "after" graph. Deltas are `right - left`.
    pub right: GraphQueryConfig,

    /// What to explore: entry points, a specific node, or all nodes.
    #[serde(default)]
    pub target: ExploreGraphTarget,

    /// Which edge structure to follow.
    #[serde(default)]
    pub graph_structure: GraphStructure,

    /// Collapse nodes that are identical on both sides. Rows then report how
    /// many unchanged nodes were skipped to reach them.
    #[serde(default)]
    pub changed_nodes_only: bool,

    /// Which columns to compute.
    /// - `None` (default): the right-hand value and `∆` for every visible view.
    /// - `Some([])`: no metrics.
    /// - `Some([...])`: exactly the listed columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[typegen(as = "Option<Vec<String>>")]
    pub metrics: Option<Vec<MetricView>>,

    /// Column to sort by. Computed for every row, even beyond the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[typegen(as = "Option<String>")]
    pub sort_by: Option<MetricView>,

    /// Sort order. Defaults to Desc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<SortOrder>,

    /// Sort `∆` columns by magnitude, so the biggest regressions and the
    /// biggest wins both surface at the top. Defaults to true, matching the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_delta_by_magnitude: Option<bool>,

    /// Skip first N results (for pagination).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,

    /// Maximum number of arrows to return. Defaults to 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// When true, populate the `ascii` field with a human-readable table
    /// (optimized for agent / LLM consumption).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ascii: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreDeltaOutput {
    /// The node being explored, with its own metrics. None when showing entry
    /// points or all nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<ExploreDeltaArrow>,
    /// Arrows to children (or to entry points / all nodes when node is None).
    pub arrows: Vec<ExploreDeltaArrow>,
    /// Metric names present in either graph.
    pub metric_names: Vec<String>,
    /// Tier names if tiered traversal is configured.
    pub tier_names: Vec<String>,
    /// Total number of arrows before offset/limit.
    pub total_arrows_count: usize,
    /// Unchanged nodes filtered out by `changed_nodes_only`. Always 0 otherwise.
    pub hidden_unchanged_count: usize,
    /// Human-readable ASCII table of the results. Only populated when
    /// `include_ascii` is set to true in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<String>,
}

/// The edge leading to a node, as it exists on one side of the twin graph.
///
/// Kept per-side rather than collapsed into the row: an edge can be *retagged*
/// (`lazy` -> `deferred`) or have its dynamic branch changed without either
/// endpoint node changing, and a single flattened `tag` would silently show
/// only the new value. Tags drive tiered traversal, so a retag is a real
/// behavioural change, not cosmetics.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreDeltaEdge {
    /// Edge tag (e.g. "lazy"), if this is a tagged edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Dynamic edge info, if this is a dynamic edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<DynamicEdgeInfo>,
}

/// One row of a delta table — `ExploreGraphArrow`'s `name` + `metrics`, plus
/// what a comparison needs: how the node changed, the edge as it exists on each
/// side, and how many unchanged nodes were collapsed to get here.
///
/// `tag` / `dynamic` live inside `l` and `r` rather than on the row, because an
/// edge can be retagged while both endpoints stay identical.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreDeltaArrow {
    /// Node name.
    pub name: String,
    /// Flat metrics map, keyed by `MetricView` display strings — e.g.
    /// `size~transitive` (right graph), `size~transitive@left`,
    /// `size~transitive@delta`.
    pub metrics: BTreeMap<String, f64>,

    /// What changed about this node between the two graphs. Bitflags:
    /// 1 = added, 2 = removed, 4 = edges changed, 8 = metrics changed.
    ///
    /// This describes the *node*. An edge that was added, removed, or retagged
    /// shows up in `l` / `r` instead — `node_diff` would put that on the edge's
    /// source, which isn't this row.
    #[typegen(as = "u32")]
    pub node_diff: NodeDiff,
    /// The edge leading here in the "before" graph. `None` means the edge is
    /// new, or that this row isn't reached via an edge (entry points, all-nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l: Option<ExploreDeltaEdge>,
    /// The edge leading here in the "after" graph. `None` means the edge was
    /// removed, or that this row isn't reached via an edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<ExploreDeltaEdge>,
    /// How many unchanged nodes were collapsed on the way to this one.
    /// 0 means a direct edge; always 0 unless `changed_nodes_only`.
    pub skipped: usize,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for ExploreDeltaInput {
    type Output = ExploreDeltaOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<ExploreDeltaOutput> {
        let ttl = Duration::from_mins(5);
        let tg = ctx
            .graph_cache
            .get_twin(&self.left, &self.right, task, ttl)
            .await?;
        let input = self;
        tokio::task::spawn_blocking(move || explore_delta(tg, &input)).await?
    }
}

// ── Sync core logic (runs in spawn_blocking) ────────────────────

fn explore_delta(tg: Arc<TwinGraph>, input: &ExploreDeltaInput) -> Result<ExploreDeltaOutput> {
    let columns = metrics::resolve_columns(&tg, &input.metrics, input.graph_structure)?;

    let resolved = rows::resolve(
        &tg,
        &input.target,
        input.graph_structure,
        input.changed_nodes_only,
    )?;
    let total_arrows_count = resolved.rows.len();

    let sort_order = input.sort_order.unwrap_or(SortOrder::Desc);
    let sorted = sort_rows(&tg, resolved.rows, input, sort_order)?;

    let offset = input.offset.unwrap_or(0);
    let limit = input.limit.unwrap_or(50);
    let page = paginate(&sorted, offset, limit);

    let arrows = page
        .iter()
        .map(|row| build_arrow(&tg, row, &columns))
        .collect::<Result<Vec<_>>>()?;
    let node = resolved
        .parent_idx
        .map(|idx| build_arrow(&tg, &rows::DeltaRow::for_node(&tg, idx), &columns))
        .transpose()?;

    let tier_names = collect_tier_names(&tg);
    let ascii = input.include_ascii.unwrap_or(false).then(|| {
        ascii::render(
            &ascii::Table {
                target: &input.target,
                graph_structure: input.graph_structure,
                changed_nodes_only: input.changed_nodes_only,
                node: node.as_ref(),
                arrows: &arrows,
                total_count: total_arrows_count,
                hidden_unchanged_count: resolved.hidden_unchanged_count,
                offset,
                sort_by_key: input.sort_by.as_ref().map(|c| c.to_string()),
                sort_order,
                tier_names: &tier_names,
            },
            tg.r.graph_settings()
                .and_then(|gs| gs.metrics_config.as_ref()),
        )
    });

    Ok(ExploreDeltaOutput {
        node,
        arrows,
        metric_names: tg.metric_names.clone(),
        tier_names,
        total_arrows_count,
        hidden_unchanged_count: resolved.hidden_unchanged_count,
        ascii,
    })
}

fn build_arrow(
    tg: &TwinGraph,
    row: &rows::DeltaRow,
    columns: &[MetricView],
) -> Result<ExploreDeltaArrow> {
    Ok(ExploreDeltaArrow {
        name: tg.merged_idx_to_name(row.node_idx).to_string(),
        metrics: metrics::build_map(tg, row.node_idx, columns)?,
        node_diff: row.node_diff,
        l: row.l.clone(),
        r: row.r.clone(),
        skipped: row.skipped,
    })
}

/// Sort by the requested column, or leave rows in natural order.
///
/// `∆` columns sort by magnitude by default: a −500 kB win is as interesting as
/// a +500 kB regression, and burying one at the far end of the table hides half
/// the story. Matches `getValuesFnForSorting` in the frontend's `metrics.tsx`.
fn sort_rows(
    tg: &TwinGraph,
    rows: Vec<rows::DeltaRow>,
    input: &ExploreDeltaInput,
    sort_order: SortOrder,
) -> Result<Vec<rows::DeltaRow>> {
    let Some(sort_by) = input.sort_by.as_ref() else {
        return Ok(rows);
    };
    let by_magnitude = sort_by.is_delta() && input.sort_delta_by_magnitude.unwrap_or(true);

    let mut valued: Vec<(f64, rows::DeltaRow)> = rows
        .into_iter()
        .map(|row| {
            let value = metrics::compute(tg, row.node_idx, sort_by)?;
            Ok((if by_magnitude { value.abs() } else { value }, row))
        })
        .collect::<Result<Vec<_>>>()?;

    valued.sort_by(|a, b| match sort_order {
        SortOrder::Asc => a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal),
        SortOrder::Desc => b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal),
    });

    Ok(valued.into_iter().map(|(_, row)| row).collect())
}

fn paginate(rows: &[rows::DeltaRow], offset: usize, limit: usize) -> &[rows::DeltaRow] {
    let start = offset.min(rows.len());
    let end = (start + limit).min(rows.len());
    &rows[start..end]
}

/// Tiers are taken from the right graph — comparing two graphs with different
/// tier configs isn't meaningful, and `get_transitive_tiered_delta` already
/// makes the same choice.
fn collect_tier_names(tg: &TwinGraph) -> Vec<String> {
    tg.r.runtime
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

impl rows::DeltaRow {
    /// The drilled-into node itself, rendered as a row so it can share the
    /// metric and formatting path with its children.
    fn for_node(tg: &TwinGraph, merged_idx: NodeIDX) -> Self {
        Self {
            node_idx: merged_idx,
            node_diff: tg.node_diff[merged_idx],
            l: None,
            r: None,
            skipped: 0,
        }
    }
}

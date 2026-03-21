// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

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

    Ok(ExploreGraphOutput {
        node,
        arrows,
        metric_names,
        tier_names,
        total_arrows_count,
    })
}

fn apply_traversal(ag: &mut ArrayGraph, gqc: &GraphQueryConfig) -> Result<()> {
    if let Some(tvc) = &gqc.traversal_config {
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

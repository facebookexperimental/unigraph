// Copyright (c) Meta Platforms, Inc. and affiliates.
use std::collections::BTreeMap;

use anyhow::Result;

use crate::ArrayGraph;
use crate::MetricView;
use crate::NodeIDX;
use crate::TieredTraversalConfig;
use crate::types::MetricName;
use crate::types::TierName;
use crate::types::array_graph::offset_graph::DFSConfigured;
use crate::types::array_graph::offset_graph::EdgeOverrides;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::array_graph::tiers::MAX_TIERS;
use crate::types::twin_graph::NodeDiff;

/// This struct is used to compute transitive deltas in delta view.
///
/// # Why do we need this?
///
/// Transitive delta value is a difference between transitive sizes of a node
/// EXCLUDING the nodes that didn't change in the graph.
///
/// Displaying delta as a simple difference between transitive sizes of a node
/// would be much simpler, but it's much less useful. For example, if we have
/// two graphs GLeft and GRight, and graph B introduces a new node that
/// already has most of the transitive dependencies from A, the delta would be
/// big (from no transitive dependencies to a lot of them) and it won't really
/// tell us much.
///
/// If we exclude non-changed nodes, we'll be able to see how much "extra"
/// stuff that node brought in.
///
/// Example of simple transitive delta:
/// ```text
/// 
///            A (t size: 3)                 A (t size: 3)
///            |                             |   D  (t size: 3)
///            |                             | /
///            B (t size: 2)                 B (t size: 2)
///            |                             |
///            C (t size: 1)                 C (t size: 1)
/// ```
/// In this graph, we added a node D. Node D has almost the same set of
/// transitive dependencies as A, so the simple transitive delta would be
///
/// Simple transitive Delta: 3 (0 on the left, 3 on the right)
/// This delta may look big, but in practice the node didn't add much to the
/// size.
///
/// Example of transitive delta excluding non-changed nodes:
/// ```text
///            A (t size: 3)                 A (t size: 3)
///            |                             |   D  (t size: 1)
///            |                             | /
///            B (t size: 2)                 B (t size: 2)
///            |                             |
///            C (t size: 1)                 C (t size: 1)
/// ```
/// In this case we exclude nodes B and C from the transitive delta
/// calculation, because then did not change.
/// This way the delta for the node D will be:
/// Delta with exclusion: 1 (0 on the left, 1 on the right, which is its self size)
pub trait ShouldCount {
    fn should_count(&self, node_idx: NodeIDX) -> bool;
}

#[derive(Clone, Copy)]
pub struct CountChangedNodesCountsForDelta<'a> {
    pub l: &'a ArrayGraph,
    pub r: &'a ArrayGraph,
    /// Which side's DFS is calling should_count.
    pub dfs_side: crate::types::twin_graph::GraphSide,
    /// Remap tables for translating between sides.
    pub remap: &'a crate::TwinRemap,
}

pub struct CountAllNodes;

impl ShouldCount for CountAllNodes {
    #[inline(always)]
    fn should_count(&self, _node_idx: NodeIDX) -> bool {
        true
    }
}

impl<'a> ShouldCount for CountChangedNodesCountsForDelta<'a> {
    fn should_count(&self, node_idx: NodeIDX) -> bool {
        use crate::types::twin_graph::GraphSide;

        let (l_unreachable, r_unreachable) = match self.dfs_side {
            GraphSide::Left => {
                let l_unreach = self.l.is_node_unreachable(node_idx);
                let merged = self.remap.l_to_twin[usize::from(node_idx)];
                let r_idx = self.remap.twin_to_r[merged];
                let r_unreach = r_idx
                    .map(|idx| self.r.is_node_unreachable(idx))
                    .unwrap_or(true);
                (l_unreach, r_unreach)
            }
            GraphSide::Right => {
                let r_unreach = self.r.is_node_unreachable(node_idx);
                let merged = self.remap.r_to_twin[usize::from(node_idx)];
                let l_idx = self.remap.twin_to_l[merged];
                let l_unreach = l_idx
                    .map(|idx| self.l.is_node_unreachable(idx))
                    .unwrap_or(true);
                (l_unreach, r_unreach)
            }
        };

        matches!(
            (l_unreachable, r_unreachable),
            (true, false) | (false, true)
        )
    }
}

#[derive(Clone, Copy)]
pub struct CountChangedNodesMetricsForDelta<'a> {
    pub l: &'a ArrayGraph,
    pub r: &'a ArrayGraph,
    pub node_diff: &'a [NodeDiff],
    pub dfs_side: crate::types::twin_graph::GraphSide,
    pub remap: &'a crate::TwinRemap,
}

impl<'a> ShouldCount for CountChangedNodesMetricsForDelta<'a> {
    fn should_count(&self, node_idx: NodeIDX) -> bool {
        use crate::types::twin_graph::GraphSide;

        // Translate to merged IDX for node_diff lookup
        let merged = match self.dfs_side {
            GraphSide::Left => self.remap.l_to_twin[usize::from(node_idx)],
            GraphSide::Right => self.remap.r_to_twin[usize::from(node_idx)],
        };

        let metric_changed = self.node_diff[merged].has_changed_metrics();
        if metric_changed {
            return true;
        }

        let (l_unreachable, r_unreachable) = match self.dfs_side {
            GraphSide::Left => {
                let l_unreach = self.l.is_node_unreachable(node_idx);
                let r_idx = self.remap.twin_to_r[merged];
                let r_unreach = r_idx
                    .map(|idx| self.r.is_node_unreachable(idx))
                    .unwrap_or(true);
                (l_unreach, r_unreach)
            }
            GraphSide::Right => {
                let r_unreach = self.r.is_node_unreachable(node_idx);
                let l_idx = self.remap.twin_to_l[merged];
                let l_unreach = l_idx
                    .map(|idx| self.l.is_node_unreachable(idx))
                    .unwrap_or(true);
                (l_unreach, r_unreach)
            }
        };

        matches!(
            (l_unreachable, r_unreachable),
            (true, false) | (false, true)
        )
    }
}

pub fn get_transitive_tiered_metric_values(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metric_name: &str,
    dominated: bool,
    should_count: impl ShouldCount,
) -> Result<BTreeMap<TierName, f64>> {
    let mut result = BTreeMap::new();

    if ag.is_node_unreachable(node_idx) || ag.node_tier_idx(node_idx).is_none() {
        return Ok(result);
    }

    let tier_config = ag
        .runtime
        .state
        .traversal_config
        .as_ref()
        .and_then(|config| config.tiered_traversal.as_ref());

    let metrics = ag.data.node_metadata.metrics.get(metric_name);

    match (metrics, tier_config) {
        (Some(metrics), Some(TieredTraversalConfig::AscendingTiers(ascending_tiers))) => {
            let mut tiered_metrics = [0.0; 8];

            let dfs_iter: Box<dyn Iterator<Item = NodeIDX>> = if dominated {
                Box::new(ag.edges_dom().dfs_configured(&[node_idx]))
            } else {
                Box::new(DFSConfigured::new(
                    &ag.data.edges.edges,
                    &ag.runtime.edge_flags,
                    &ag.data.edges.edge_offsets,
                    &[node_idx],
                ))
            };

            for node_idx in dfs_iter {
                if should_count.should_count(node_idx) {
                    let value = metrics[node_idx];
                    let tier_idx = ag.try_node_tier_idx(node_idx)?;
                    tiered_metrics[tier_idx] += value;
                }
            }
            // Make tiered metrics cumulative. meaning that every next tier has
            // its own value plus the previous tier's value combined
            // T1_cml = T1, T2_cml = T1 + T2, T3_cml = T1 + T2 + T3, etc.
            let mut cumulative = 0.0;
            for (tier_idx, tier) in ascending_tiers.tiers.iter().enumerate() {
                let value = tiered_metrics[tier_idx];
                let cml_value = cumulative + value;
                cumulative = cml_value;
                if cml_value > 0.0 {
                    result.insert(tier.name.to_string(), cml_value);
                }
            }

            Ok(result)
        }
        (None, _) => Ok(result),
        (_, None) => Ok(result),
    }
}

/// Resolve a single [`MetricView`] to a number for one node.
///
/// The only place that maps a user-facing metric view onto the accessor that
/// produces it — every caller that renders metric columns goes through here so
/// the views cannot drift apart.
pub fn metric_value(ag: &ArrayGraph, node_idx: NodeIDX, view: &MetricView) -> Result<f64> {
    match view {
        MetricView::Metric { name, .. } => Ok(ag
            .data
            .node_metadata
            .metrics
            .get(name.as_str())
            .map_or(0.0, |values| values[node_idx])),
        MetricView::Transitive { name, .. } => {
            ag.get_transitive_metric_value(node_idx, name, false)
        }
        MetricView::Dominated { name, .. } => ag.get_transitive_metric_value(node_idx, name, true),
        MetricView::Tiered {
            name, tier_name, ..
        } => {
            let tiered = ag.get_transitive_tiered_metric_values(node_idx, name, false)?;
            Ok(*tiered.get(tier_name.as_str()).unwrap_or(&0.0))
        }
        MetricView::TieredDominated {
            name, tier_name, ..
        } => {
            let tiered = ag.get_transitive_tiered_metric_values(node_idx, name, true)?;
            Ok(*tiered.get(tier_name.as_str()).unwrap_or(&0.0))
        }
        MetricView::ParentsCount { .. } => Ok(ag.parents_len_configured(node_idx) as f64),
        MetricView::CountTransitive { .. } => Ok(ag.transitive_count_configured(node_idx) as f64),
        MetricView::CountDominated { .. } => {
            Ok(ag.transitive_count_configured_dominated(node_idx) as f64)
        }
        MetricView::TierIndex { .. } => Ok(ag.node_tier_idx(node_idx).unwrap_or(0) as f64),
    }
}

/// How many parents does this node have in the conrigured traversal?
/// NOTE: the edge might be included, but if the parent node is unreachable,
/// we won't count it.
pub fn parents_len_configured(ag: &ArrayGraph, node_idx: NodeIDX) -> usize {
    if ag.is_node_unreachable(node_idx) {
        return 0;
    }

    let mut count = 0;
    for (target, flags) in ag.edges_reverse().edges(node_idx) {
        if flags.contains(EdgeFlags::EXCLUDED) {
            continue;
        }

        if ag.runtime.node_flags[target].is_node_unreachable() {
            continue;
        }
        count += 1;
    }

    count
}

pub fn get_transitive_metric_value(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metric_name: &str,
    dominated: bool,
) -> Result<f64> {
    if ag.is_node_unreachable(node_idx) {
        return Ok(0.0);
    }

    let dfs_iter: Box<dyn Iterator<Item = NodeIDX>> = if dominated {
        Box::new(ag.edges_dom().dfs_configured(&[node_idx]))
    } else {
        Box::new(DFSConfigured::new(
            &ag.data.edges.edges,
            &ag.runtime.edge_flags,
            &ag.data.edges.edge_offsets,
            &[node_idx],
        ))
    };

    let mut total = 0.0;
    if let Some(metrics) = ag.data.node_metadata.metrics.get(metric_name) {
        for node_idx in dfs_iter {
            let value = metrics[node_idx];
            total += value
        }
        Ok(total)
    } else {
        Ok(0.0)
    }
}

/// Min and max of a metric across ALL nodes (reachable or not), in a single
/// O(N) walk. Used by the UI to position timespan bars on a shared timeline.
/// When `ignore_zero` is set, `0.0` values (the default for missing metrics)
/// are excluded so they don't drag the range to zero.
/// Returns `None` when the metric is absent or has no qualifying values.
pub fn metric_min_max(ag: &ArrayGraph, metric_name: &str, ignore_zero: bool) -> Option<(f64, f64)> {
    let values = ag.data.node_metadata.metrics.get(metric_name)?;
    values
        .iter()
        .filter(|&&v| !(ignore_zero && v == 0.0))
        .fold(None, |acc, &v| match acc {
            None => Some((v, v)),
            Some((min, max)) => Some((min.min(v), max.max(v))),
        })
}

pub fn get_metrics_sums_for_nodes(
    ag: &ArrayGraph,
    node_idxs: &[NodeIDX],
) -> Result<BTreeMap<String, f64>> {
    let mut result = BTreeMap::new();

    for (metric_name, metrics) in &ag.data.node_metadata.metrics {
        let mut total = 0.0;

        for node_idx in node_idxs {
            if ag.is_node_unreachable(*node_idx) {
                continue;
            }

            let value = metrics[*node_idx];
            total += value;
        }

        result.insert(metric_name.to_string(), total);
    }

    Ok(result)
}

/// This returns summed up metric for provided nodes.
/// it DOES NOT compute tiers, it takes the tiers that are already assigned to the
/// nodes and aggregates the metrics for each tier.
/// The use case of it is to show how combined metrics for selected nodes.
pub fn get_metrics_sums_tiered_for_nodes(
    ag: &ArrayGraph,
    node_idxs: &[NodeIDX],
) -> Result<BTreeMap<MetricName, BTreeMap<TierName, f64>>> {
    let mut result: BTreeMap<MetricName, BTreeMap<TierName, f64>> = BTreeMap::new();

    let tier_config = ag
        .runtime
        .state
        .traversal_config
        .as_ref()
        .and_then(|config| config.tiered_traversal.as_ref());

    if let Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)) = tier_config {
        for (metric_name, metrics) in &ag.data.node_metadata.metrics {
            let mut tiered_metrics = [0.0; MAX_TIERS];

            for node_idx in node_idxs {
                if ag.is_node_unreachable(*node_idx) {
                    continue;
                }

                let tier_idx = ag.try_node_tier_idx(*node_idx)?;
                let value = metrics[*node_idx];

                // make it cumulative.
                #[allow(clippy::needless_range_loop)]
                for add_metric_to_tier_idx in tier_idx..MAX_TIERS {
                    tiered_metrics[add_metric_to_tier_idx] += value;
                }
            }

            for (tier_idx, value) in tiered_metrics.iter().enumerate() {
                if *value > 0.0
                    && let Some(tier) = ascending_tiers.tiers.get(tier_idx)
                {
                    let tier_name = tier.name.to_string();
                    result
                        .entry(metric_name.to_string())
                        .or_default()
                        .insert(tier_name, *value);
                }
            }
        }
    }

    Ok(result)
}

/// Get Tiered metrics for the set of entry points (which normally means
/// the whole reachable graph).
///
/// `overrides` lets callers run a "what-if" traversal that force-includes or
/// force-excludes specific edges without mutating the graph's edge flags. Pass
/// `&EdgeOverrides::default()` for the plain traversal.
pub fn get_combined_metrics_for_entry_points(
    ag: &ArrayGraph,
    overrides: &EdgeOverrides,
) -> Result<CombinedMetricsForNodes> {
    let mut tiered_result = BTreeMap::new();
    let mut metric_result = BTreeMap::new();
    let mut node_count = 0;

    let tier_config = ag
        .runtime
        .state
        .traversal_config
        .as_ref()
        .and_then(|config| config.tiered_traversal.as_ref());

    // get the indexed vec for metrics so we don't acess maps with a string key
    // on every iteration.
    let metric_names = ag
        .data
        .node_metadata
        .metrics
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let metric_values = ag
        .data
        .node_metadata
        .metrics
        .values()
        .collect::<Vec<&Vec<f64>>>();

    let mut tiered_result_vec: Vec<[f64; MAX_TIERS]> = metric_names
        .iter()
        .map(|_name| [0.0; MAX_TIERS])
        .collect::<Vec<_>>();

    let mut metrics_result_vec = vec![0.0; metric_names.len()];

    let ascending_tiers = match tier_config {
        Some(TieredTraversalConfig::AscendingTiers(at)) => Some(at),
        _ => None,
    };

    let entry_points = ag.determine_entrypoints();

    // Single traversal loop — tiered DFS when tiers are configured,
    // plain DFS otherwise. Both produce (node_idx, Option<tier_idx>).
    if let Some(ascending_tiers) = ascending_tiers {
        let iter = crate::traversal::tiered_traversal::TieredTraversalIter::new_with_overrides(
            &ag.data.edges.edges,
            &ag.runtime.edge_flags,
            &ag.data.edges.edge_offsets,
            &ascending_tiers.tiers,
            &entry_points,
            Some(overrides),
        );
        for next in iter {
            let (node_idx, tier_idx) = next?;
            node_count += 1;

            for (metric_idx, _) in metric_names.iter().enumerate() {
                let v = metric_values[metric_idx][node_idx];
                metrics_result_vec[metric_idx] += v;
                for item in &mut tiered_result_vec[metric_idx][tier_idx..] {
                    *item += v;
                }
            }
        }
    } else {
        for node_idx in DFSConfigured::new_with_overrides(
            &ag.data.edges.edges,
            &ag.runtime.edge_flags,
            &ag.data.edges.edge_offsets,
            &entry_points,
            Some(overrides),
        ) {
            node_count += 1;

            for (metric_idx, _) in metric_names.iter().enumerate() {
                metrics_result_vec[metric_idx] += metric_values[metric_idx][node_idx];
            }
        }
    }

    // Build flat metrics result (always)
    for (metric_idx, metric_name) in metric_names.iter().enumerate() {
        metric_result.insert(metric_name.clone(), metrics_result_vec[metric_idx]);
    }

    // Build tiered metrics result (only when tiers are configured)
    if let Some(ascending_tiers) = ascending_tiers {
        for (metric_idx, tiered_metrics) in tiered_result_vec.iter().enumerate() {
            let metric_name = &metric_names[metric_idx];
            let mut tiered_map = BTreeMap::new();

            for (tier_idx, value) in tiered_metrics.iter().enumerate() {
                if *value > 0.0
                    && let Some(tier) = ascending_tiers.tiers.get(tier_idx)
                {
                    tiered_map.insert(tier.name.to_string(), *value);
                }
            }

            tiered_result.insert(metric_name.clone(), tiered_map);
        }
    }

    Ok(CombinedMetricsForNodes {
        metrics: metric_result,
        tiered_metrics: tiered_result,
        node_count,
    })
}

/// Represents values for metrics for a set of nodes.
/// Not transitive, just aggregated for things like
/// "give me total size of all the nodes i just selected"
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone, Debug)]
pub struct CombinedMetricsForNodes {
    pub metrics: BTreeMap<MetricName, f64>,
    pub tiered_metrics: BTreeMap<MetricName, BTreeMap<TierName, f64>>,
    pub node_count: usize,
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::TraversalConfig;
    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::name_to_idx;
    use crate::tests::test_utils::print_forward_edges;
    use crate::tests::test_utils::traversal_config_test_trait::TraversalConfigTestTrait;

    #[test]
    fn test_metric_min_max() -> Result<()> {
        // Distinct values, and C is not reachable from A — min/max folds over
        // ALL nodes regardless of reachability.
        // D has no event_start metric, so it defaults to 0.0.
        let json = r#"{
          "nodes": {
            "A": { "metrics": { "event_start": 10.0 }, "edges_directed": ["B", "D"] },
            "B": { "metrics": { "event_start": 5.0 }, "edges_directed": [] },
            "C": { "metrics": { "event_start": 25.0 }, "edges_directed": [] },
            "D": { "metrics": { "size": 1.0 }, "edges_directed": [] }
          }
        }"#;
        let ag = crate::MapGraph::from_json(json)?.to_array_graph(&ll::Task::create_new("test"))?;

        // Without ignore_zero, D's default 0.0 drags the min down.
        assert_eq!(ag.metric_min_max("event_start", false), Some((0.0, 25.0)));
        // With ignore_zero, the missing-metric 0.0 is excluded.
        assert_eq!(ag.metric_min_max("event_start", true), Some((5.0, 25.0)));
        assert_eq!(ag.metric_min_max("missing", false), None);
        Ok(())
    }

    #[test]
    fn metrics_with_overrides() -> Result<()> {
        let mut ag = make_test_array_graph_2()?;
        let mut tvc = TraversalConfig::default();
        tvc.with_tier_config();
        tvc.set_force_node("F", false);

        ag.apply_traversal_config_and_entry_points(tvc)?;

        snapshot!(
            print_forward_edges(&ag),
            "
A -> B
A -> D
B -> C [T]
B -> J [T]
D -> F
D -> E [T]
E -> K
F -> G [D]
F -> H [D]
F -> I [D]
J -> K
L -> D
L -> M
M -> O
N -> M
O -> N
O -> P
O -> F [T]
"
        );

        // Node `F` is forced out, so the edges D->F and O->F are EXCLUDED and
        // P is reachable only via O->P. Each case overlays edge overrides on
        // top of these flags WITHOUT mutating the graph, then re-measures.
        let (o, f, p) = (
            name_to_idx(&ag, "O"),
            name_to_idx(&ag, "F"),
            name_to_idx(&ag, "P"),
        );

        let cases: Vec<(&str, EdgeOverrides)> = vec![
            ("baseline (no overrides)", EdgeOverrides::default()),
            // Force-include the excluded, tagged O->F edge: pulls F + its
            // dominated subtree (G, H, I) in at tier T4.
            ("include O->F", EdgeOverrides::from_triplets([(o, f, true)])),
            // Force-exclude the normally-included O->P edge: drops P (T1).
            (
                "exclude O->P",
                EdgeOverrides::from_triplets([(o, p, false)]),
            ),
            // Multiple overrides at once, mixing include + exclude.
            (
                "include O->F + exclude O->P",
                EdgeOverrides::from_triplets([(o, f, true), (o, p, false)]),
            ),
        ];

        let mut results = Vec::with_capacity(cases.len());
        for (label, overrides) in &cases {
            results.push((*label, ag.get_combined_metrics_for_entry_points(overrides)?));
        }

        snapshot!(
            format_metrics_table(&results),
            "
case                         nodes   size     T1     T2     T3     T4
baseline (no overrides)         12     12      8     10     11     12
include O->F                    16     16      8     10     11     16
exclude O->P                    11     11      7      9     10     11
include O->F + exclude O->P     15     15      7      9     10     15

"
        );

        Ok(())
    }

    /// Render combined-metrics results (one row per case) as a single ASCII
    /// table so all inputs/outputs are visible in one snapshot.
    fn format_metrics_table(results: &[(&str, CombinedMetricsForNodes)]) -> String {
        let tier = |r: &CombinedMetricsForNodes, name: &str| -> f64 {
            r.tiered_metrics
                .get("size")
                .and_then(|t| t.get(name))
                .copied()
                .unwrap_or(0.0)
        };

        let mut out = String::new();
        out.push_str(&format!(
            "{:<28} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6}\n",
            "case", "nodes", "size", "T1", "T2", "T3", "T4"
        ));
        for (label, r) in results {
            let size = r.metrics.get("size").copied().unwrap_or(0.0);
            out.push_str(&format!(
                "{:<28} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6}\n",
                label,
                r.node_count,
                size,
                tier(r, "T1"),
                tier(r, "T2"),
                tier(r, "T3"),
                tier(r, "T4"),
            ));
        }
        out
    }
}

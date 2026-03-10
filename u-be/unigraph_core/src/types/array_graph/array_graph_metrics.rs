use std::collections::BTreeMap;

use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::TieredTraversalConfig;
use crate::types::MetricName;
use crate::types::TierName;
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
        match (
            self.l.is_node_unreachable(node_idx),
            self.r.is_node_unreachable(node_idx),
        ) {
            // was unreachable and is unreachable. not interesting to us. this
            // technically shouldn't even happen
            (true, true) => false,
            // was reachable and is reachable. not interesting to us
            (false, false) => false,

            // if reachability changed, we do want to count it
            (true, false) => true,
            (false, true) => true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct CountChangedNodesMetricsForDelta<'a> {
    pub l: &'a ArrayGraph,
    pub r: &'a ArrayGraph,
    pub node_diff: &'a [NodeDiff],
}

impl<'a> ShouldCount for CountChangedNodesMetricsForDelta<'a> {
    fn should_count(&self, node_idx: NodeIDX) -> bool {
        let metric_changed = self.node_diff[node_idx].has_changed_metrics();
        if metric_changed {
            // we always want to count nodes that had their metrics changed
            return true;
        }

        match (
            self.l.is_node_unreachable(node_idx),
            self.r.is_node_unreachable(node_idx),
        ) {
            // was unreachable and is unreachable. not interesting to us. this
            // technically shouldn't even happen
            (true, true) => false,
            // was reachable and is reachable. not interesting to us
            (false, false) => false,

            // if reachability changed, we do want to count it
            (true, false) => true,
            (false, true) => true,
        }
    }
}

pub fn get_transitive_tiered_metric_values(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metric_name: &str,
    dominated: bool,
    should_count: impl ShouldCount,
) -> Result<BTreeMap<TierName, f32>> {
    let mut result = BTreeMap::new();

    if ag.is_node_unreachable(node_idx) || ag.node_tier_idx(node_idx).is_none() {
        return Ok(result);
    }

    let tier_config = ag
        .state
        .traversal_config
        .as_ref()
        .and_then(|config| config.tiered_traversal.as_ref());

    let metrics = ag.metrics.get(metric_name);

    match (metrics, tier_config) {
        (Some(metrics), Some(TieredTraversalConfig::AscendingTiers(ascending_tiers))) => {
            let mut tiered_metrics = [0.0; 8];

            let edges = if dominated {
                ag.edges_dom()
            } else {
                &ag.edges_forward
            };

            for node_idx in edges.dfs_configured(&[node_idx]) {
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

/// How many parents does this node have in the conrigured traversal?
/// NOTE: the edge might be included, but if the parent node is unreachable,
/// we won't count it.
pub fn parents_len_configured(ag: &ArrayGraph, node_idx: NodeIDX) -> usize {
    if ag.is_node_unreachable(node_idx) {
        return 0;
    }

    let mut count = 0;
    for edge in ag.derived_state.edges_reverse.edges(node_idx) {
        if edge.flags.is_excluded() {
            continue;
        }

        if ag.node_flags[edge.points_to].is_node_unreachable() {
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
) -> Result<f32> {
    if ag.is_node_unreachable(node_idx) {
        return Ok(0.0);
    }

    let edges = if dominated {
        ag.edges_dom()
    } else {
        &ag.edges_forward
    };

    let mut total = 0.0;
    if let Some(metrics) = ag.metrics.get(metric_name) {
        for node_idx in edges.dfs_configured(&[node_idx]) {
            let value = metrics[node_idx];
            total += value
        }
        Ok(total)
    } else {
        Ok(0.0)
    }
}

pub fn get_metrics_sums_for_nodes(
    ag: &ArrayGraph,
    node_idxs: &[NodeIDX],
) -> Result<BTreeMap<String, f32>> {
    let mut result = BTreeMap::new();

    for (metric_name, metrics) in &ag.metrics {
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
) -> Result<BTreeMap<MetricName, BTreeMap<TierName, f32>>> {
    let mut result: BTreeMap<MetricName, BTreeMap<TierName, f32>> = BTreeMap::new();

    let tier_config = ag
        .state
        .traversal_config
        .as_ref()
        .and_then(|config| config.tiered_traversal.as_ref());

    if let Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)) = tier_config {
        for (metric_name, metrics) in &ag.metrics {
            let mut tiered_metrics = [0.0; 4];

            for node_idx in node_idxs {
                if ag.is_node_unreachable(*node_idx) {
                    continue;
                }

                let tier_idx = ag.try_node_tier_idx(*node_idx)?;
                let value = metrics[*node_idx];

                // make it cumulative.
                #[allow(clippy::needless_range_loop)]
                for add_metric_to_tier_idx in tier_idx..4 {
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
pub fn get_combined_metrics_for_entry_points(
    ag: &mut ArrayGraph,
    force_edge_include: Option<(NodeIDX, NodeIDX)>,
) -> Result<CombinedMetricsForNodes> {
    let mut tiered_result = BTreeMap::new();
    let mut metric_result = BTreeMap::new();
    let mut node_count = 0;

    let edge_override = if let Some((from, to)) = force_edge_include {
        // if we have a forced edge, we need to include it in the graph
        // and make sure it's not excluded.
        ag.edges_forward.override_edge_force_include(from, to)
    } else {
        None
    };

    let tier_config = ag
        .state
        .traversal_config
        .as_ref()
        .and_then(|config| config.tiered_traversal.as_ref());

    // get the indexed vec for metrics so we don't acess maps with a string key
    // on every iteration.
    let metric_names = ag.metrics.keys().cloned().collect::<Vec<_>>();
    let metric_values = ag.metrics.values().collect::<Vec<&Vec<f32>>>();

    let mut tiered_result_vec: Vec<[f32; 4]> = metric_names
        .iter()
        .map(|_name| [0.0; 4])
        .collect::<Vec<_>>();

    let mut metrics_result_vec = vec![0.0; metric_names.len()];

    if let Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)) = tier_config {
        let entry_points = ag.determine_entrypoints();

        for next in ag
            .edges_forward
            .dfs_tiered_configured(&ascending_tiers.tiers, &entry_points)?
        {
            let (node_idx, tier_idx) = next?;
            node_count += 1;

            //
            for (metric_idx, _metric_name) in metric_names.iter().enumerate() {
                let metric_value = metric_values[metric_idx][node_idx];

                metrics_result_vec[metric_idx] += metric_value;

                // make it cumulative. If the node appears on one tier, it's also
                // included in all tiers above it.
                for item in &mut tiered_result_vec[metric_idx][tier_idx..] {
                    *item += metric_value;
                }
            }
        }

        for (metric_idx, tiered_metrics) in tiered_result_vec.iter().enumerate() {
            let metric_name = &metric_names[metric_idx];

            metric_result.insert(metric_name.clone(), metrics_result_vec[metric_idx]);

            let mut tiered_map = BTreeMap::new();

            for (tier_idx, value) in tiered_metrics.iter().enumerate() {
                if *value > 0.0
                    && let Some(tier) = ascending_tiers.tiers.get(tier_idx)
                {
                    let tier_name = tier.name.to_string();
                    tiered_map.insert(tier_name, *value);
                }
            }

            tiered_result.insert(metric_name.clone(), tiered_map);
        }
    }

    if let Some(edge_override) = edge_override {
        // restore the edge override
        ag.edges_forward.restore_edge_override(edge_override);
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
    pub metrics: BTreeMap<MetricName, f32>,
    pub tiered_metrics: BTreeMap<MetricName, BTreeMap<TierName, f32>>,
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
    fn metrics_with_overrides() -> Result<()> {
        let mut ag = make_test_array_graph_2()?;
        let mut tvc = TraversalConfig::default();
        tvc.with_tier_config();
        tvc.set_force_node("F", false);

        ag.apply_traversal_config(tvc)?;

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

        let original_r = ag.get_combined_metrics_for_entry_points(None)?;

        snapshot!(
            &original_r,
            r#"
CombinedMetricsForNodes {
    metrics: {
        "size": 12.0,
    },
    tiered_metrics: {
        "size": {
            "T1": 8.0,
            "T2": 10.0,
            "T3": 11.0,
            "T4": 12.0,
        },
    },
    node_count: 12,
}
"#
        );

        let overridden_r = ag.get_combined_metrics_for_entry_points(Some((
            name_to_idx(&ag, "O"),
            name_to_idx(&ag, "F"),
        )))?;

        snapshot!(
            overridden_r,
            r#"
CombinedMetricsForNodes {
    metrics: {
        "size": 16.0,
    },
    tiered_metrics: {
        "size": {
            "T1": 8.0,
            "T2": 10.0,
            "T3": 11.0,
            "T4": 16.0,
        },
    },
    node_count: 16,
}
"#
        );

        Ok(())
    }
}

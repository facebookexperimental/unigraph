use std::collections::BTreeMap;

use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::TieredTraversalConfig;
use crate::types::MetricName;
use crate::types::TierName;

pub fn get_transitive_tiered_metric_values(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metric_name: &str,
    dominated: bool,
) -> Result<BTreeMap<TierName, f32>> {
    let mut result = BTreeMap::new();

    if ag.is_node_unreachable(node_idx) || ag.node_tier_idx(node_idx).is_none() {
        return Ok(result);
    }

    let tier_config = ag
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
                let value = metrics[node_idx];
                let tier_idx = ag.try_node_tier_idx(node_idx)?;
                tiered_metrics[tier_idx] += value;
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
) -> Result<f32> {
    if ag.is_node_unreachable(node_idx) {
        return Ok(0.0);
    }

    let mut total = 0.0;
    if let Some(metrics) = ag.metrics.get(metric_name) {
        for node_idx in ag.edges_forward.dfs_configured(&[node_idx]) {
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
                if *value > 0.0 {
                    let tier_name = ascending_tiers.tiers[tier_idx].name.to_string();
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
                for add_metric_to_tier_idx in tier_idx..4 {
                    tiered_result_vec[metric_idx][add_metric_to_tier_idx] += metric_value;
                }
            }
        }

        for (metric_idx, tiered_metrics) in tiered_result_vec.iter().enumerate() {
            let metric_name = &metric_names[metric_idx];

            metric_result.insert(metric_name.clone(), metrics_result_vec[metric_idx]);

            let mut tiered_map = BTreeMap::new();

            for (tier_idx, value) in tiered_metrics.iter().enumerate() {
                if *value > 0.0 {
                    let tier_name = ascending_tiers.tiers[tier_idx].name.to_string();
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
#[derive(serde::Serialize, serde::Deserialize, ts_rs::TS, Clone, Debug)]
#[ts(export)]
pub struct CombinedMetricsForNodes {
    pub metrics: BTreeMap<MetricName, f32>,
    pub tiered_metrics: BTreeMap<MetricName, BTreeMap<TierName, f32>>,
    pub node_count: usize,
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::Decision;
    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_graphs::traversal_config_with_tiers;
    use crate::tests::test_utils::name_to_idx;
    use crate::tests::test_utils::print_forward_edges;

    #[test]
    fn metrics_with_overrides() -> Result<()> {
        let mut ag = make_test_array_graph_2()?;
        let mut tvc = traversal_config_with_tiers();
        tvc.force_nodes.insert(
            "F".into(),
            Decision {
                include: false,
                message: None,
            },
        );

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

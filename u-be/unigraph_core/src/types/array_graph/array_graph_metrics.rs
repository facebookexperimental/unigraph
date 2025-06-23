use std::collections::BTreeMap;

use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::TieredTraversalConfig;
use crate::types::TierName;

pub fn get_transitive_tiered_metric_values(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    metric_name: &str,
    dominated: bool,
) -> Result<BTreeMap<TierName, f32>> {
    let mut result = BTreeMap::new();

    if ag.is_node_unreachable(node_idx) {
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
                let tier_idx = ag.node_flags[node_idx].tier_idx();
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
    for edge in ag.edges_reverse.edges(node_idx) {
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

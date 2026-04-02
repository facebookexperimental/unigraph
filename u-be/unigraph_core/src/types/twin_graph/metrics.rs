// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::GraphSide;
use crate::NodeIDX;
use crate::TwinGraph;
use crate::types::array_graph::array_graph_metrics::CountChangedNodesCountsForDelta;
use crate::types::array_graph::array_graph_metrics::CountChangedNodesMetricsForDelta;
use crate::types::array_graph::array_graph_metrics::ShouldCount;
use crate::types::array_graph::get_transitive_tiered_metric_values;

/// Transitive delta value is a difference between transitive sizes of a node
/// EXCLUDING the nodes that didn't change in the graph.
///
/// Displaying delta as a simple difference between transitive sizes of a node
/// would be much simpler, but it's much less useful. For example, if we have
/// two grapha GLeft and GRight, and graph B introduces a new node that
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
pub fn get_transitive_count_delta(
    l: &ArrayGraph,
    r: &ArrayGraph,
    node_idx: NodeIDX,
) -> Result<i32> {
    if l.is_node_unreachable(node_idx) && r.is_node_unreachable(node_idx) {
        return Ok(0);
    }

    let should_count = CountChangedNodesCountsForDelta { l, r };

    let count_l = if l.node_exists(node_idx) {
        l.edges_forward
            .dfs_configured(&[node_idx])
            .filter(|node_idx| should_count.should_count(*node_idx))
            .count()
    } else {
        0
    };

    let count_r = if r.node_exists(node_idx) {
        r.edges_forward
            .dfs_configured(&[node_idx])
            .filter(|node_idx| should_count.should_count(*node_idx))
            .count()
    } else {
        0
    };

    Ok(count_r as i32 - count_l as i32)
}

pub fn get_transitive_tiered_delta(
    tg: &TwinGraph,
    node_idx: NodeIDX,
    metric_name: &str,
) -> Result<BTreeMap<String, f32>> {
    {
        if tg.l.is_none() {
            return Ok(BTreeMap::new());
        }

        let l = tg.graph(GraphSide::Left)?;
        let r = &tg.r;
        let should_count = CountChangedNodesMetricsForDelta {
            l,
            r,
            node_diff: &tg.node_diff,
        };
        let result_l =
            get_transitive_tiered_metric_values(l, node_idx, metric_name, false, should_count)?;
        let result_r =
            get_transitive_tiered_metric_values(r, node_idx, metric_name, false, should_count)?;

        // comparing two graphs with different tiers is probably tricky.
        // for now we'll just take tiers from the right graph.
        let tiers = &r.state.tiers;

        anyhow::Ok(
            tiers
                .iter()
                .map(|(tier_name, _idx)| {
                    let l_value = *result_l.get(tier_name).unwrap_or(&0.0);
                    let r_value = *result_r.get(tier_name).unwrap_or(&0.0);
                    (tier_name.clone(), r_value - l_value)
                })
                .collect::<BTreeMap<_, _>>(),
        )
    }
    .context("get_transitive_tiered_delta")
}

#[cfg(test)]
mod tests {
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_graphs::make_twin_graph_with_tier_config;

    #[test]
    fn test_get_transitive_count_delta() -> Result<()> {
        let tg = make_twin_graph()?;
        let t_idx = tg.node_names.name_to_idx_log("T").unwrap();

        let t_left = tg.l.as_ref().unwrap().transitive_count_configured(t_idx);
        let t_right = tg.r.transitive_count_configured(t_idx);
        let t_delta = tg.get_transitive_count_delta(t_idx)?;

        assert_equal!(t_left, 0);
        assert_equal!(t_right, 8);
        assert_equal!(t_delta, 1); // this is 1 (!!!), not 8. Because only one node was added with 7 existing deps
        Ok(())
    }

    #[test]
    fn test_get_transitive_tiered_delta() -> Result<()> {
        let tg = make_twin_graph_with_tier_config()?;
        let l = tg.graph(GraphSide::Left)?;
        let r = tg.graph(GraphSide::Right)?;
        let m = "size";

        let a_idx = tg.node_names.name_to_idx_log("A").unwrap();
        let value_l = l.get_transitive_tiered_metric_values(a_idx, m, false)?;
        let value_r = r.get_transitive_tiered_metric_values(a_idx, m, false)?;
        let delta = tg.get_transitive_tiered_delta(a_idx, m)?;

        snapshot!(
            ("Left:", value_l, "Right:", value_r, "Delta:", delta),
            r#"
(
    "Left:",
    {
        "T1": 7.0,
        "T2": 9.0,
        "T3": 10.0,
        "T4": 11.0,
    },
    "Right:",
    {
        "T1": 9.0,
        "T2": 15.0,
        "T3": 15.0,
        "T4": 16.0,
    },
    "Delta:",
    {
        "T1": 2.0,
        "T2": 5.0,
        "T3": 5.0,
        "T4": 5.0,
    },
)
"#
        );

        let t_idx = tg.node_names.name_to_idx_log("T").unwrap();
        let value_l = l.get_transitive_tiered_metric_values(t_idx, m, false)?;
        let value_r = r.get_transitive_tiered_metric_values(t_idx, m, false)?;
        let delta = tg.get_transitive_tiered_delta(t_idx, m)?;

        snapshot!(
            ("Left:", value_l, "Right:", value_r, "Delta:", delta),
            r#"
(
    "Left:",
    {},
    "Right:",
    {
        "T1": 6.0,
        "T2": 8.0,
        "T3": 8.0,
        "T4": 8.0,
    },
    "Delta:",
    {
        "T1": 1.0,
        "T2": 1.0,
        "T3": 1.0,
        "T4": 1.0,
    },
)
"#
        );

        Ok(())
    }
}

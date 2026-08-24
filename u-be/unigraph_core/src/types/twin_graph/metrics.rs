// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::TwinGraph;
use crate::types::array_graph::array_graph_metrics::CountChangedNodesCountsForDelta;
use crate::types::array_graph::array_graph_metrics::CountChangedNodesMetricsForDelta;
use crate::types::array_graph::array_graph_metrics::ShouldCount;
use crate::types::array_graph::get_transitive_tiered_metric_values;
use crate::types::twin_graph::GraphSide;

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
    tg: &TwinGraph,
    l: &ArrayGraph,
    r: &ArrayGraph,
    merged_idx: NodeIDX,
) -> Result<i32> {
    let l_idx = tg.to_local(GraphSide::Left, merged_idx);
    let r_idx = tg.to_local(GraphSide::Right, merged_idx);

    let l_unreachable = l_idx.is_none_or(|idx| l.is_node_unreachable(idx));
    let r_unreachable = r_idx.is_none_or(|idx| r.is_node_unreachable(idx));

    if l_unreachable && r_unreachable {
        return Ok(0);
    }

    let remap = &tg.remap;

    let should_count_l = CountChangedNodesCountsForDelta {
        l,
        r,
        dfs_side: GraphSide::Left,
        remap,
    };
    let should_count_r = CountChangedNodesCountsForDelta {
        l,
        r,
        dfs_side: GraphSide::Right,
        remap,
    };

    let count_l = match l_idx {
        Some(idx) if !l.is_node_unreachable(idx) => l
            .forward_edge_view()
            .dfs_configured(&[idx])
            .filter(|node_idx| should_count_l.should_count(*node_idx))
            .count(),
        _ => 0,
    };

    let count_r = match r_idx {
        Some(idx) if !r.is_node_unreachable(idx) => r
            .forward_edge_view()
            .dfs_configured(&[idx])
            .filter(|node_idx| should_count_r.should_count(*node_idx))
            .count(),
        _ => 0,
    };

    Ok(count_r as i32 - count_l as i32)
}

pub fn get_transitive_tiered_delta(
    tg: &TwinGraph,
    merged_idx: NodeIDX,
    metric_name: &str,
) -> Result<BTreeMap<String, f64>> {
    {
        let l = tg.graph(GraphSide::Left);
        let r = &tg.r;
        let remap = &tg.remap;

        let l_idx = tg.to_local(GraphSide::Left, merged_idx);
        let r_idx = tg.to_local(GraphSide::Right, merged_idx);

        let should_count_l = CountChangedNodesMetricsForDelta {
            l,
            r,
            node_diff: &tg.node_diff,
            dfs_side: GraphSide::Left,
            remap,
        };
        let should_count_r = CountChangedNodesMetricsForDelta {
            l,
            r,
            node_diff: &tg.node_diff,
            dfs_side: GraphSide::Right,
            remap,
        };

        let result_l = match l_idx {
            Some(idx) => {
                get_transitive_tiered_metric_values(l, idx, metric_name, false, should_count_l)?
            }
            None => BTreeMap::new(),
        };
        let result_r = match r_idx {
            Some(idx) => {
                get_transitive_tiered_metric_values(r, idx, metric_name, false, should_count_r)?
            }
            None => BTreeMap::new(),
        };

        // comparing two graphs with different tiers is probably tricky.
        // for now we'll just take tiers from the right graph.
        let tiers = &r.runtime.state.tiers;

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
        let t_r_idx = tg.r.data.node_names_ordered.name_to_idx_log("T").unwrap();
        let t_merged = tg.to_merged(GraphSide::Right, t_r_idx);

        let t_left_local = tg.to_local(GraphSide::Left, t_merged);
        let t_left = match t_left_local {
            Some(idx) => tg.l.transitive_count_configured(idx),
            None => 0,
        };
        let t_right = tg.r.transitive_count_configured(t_r_idx);
        let t_delta = tg.get_transitive_count_delta(t_merged)?;

        assert_equal!(t_left, 0);
        assert_equal!(t_right, 8);
        assert_equal!(t_delta, 1); // this is 1 (!!!), not 8. Because only one node was added with 7 existing deps
        Ok(())
    }

    #[test]
    fn test_get_transitive_tiered_delta() -> Result<()> {
        let tg = make_twin_graph_with_tier_config()?;
        let l = tg.graph(GraphSide::Left);
        let r = tg.graph(GraphSide::Right);
        let m = "size";

        let a_r_idx = r.data.node_names_ordered.name_to_idx_log("A").unwrap();
        let a_merged = tg.to_merged(GraphSide::Right, a_r_idx);
        let a_l_idx = tg.to_local(GraphSide::Left, a_merged).unwrap();
        let value_l = l.get_transitive_tiered_metric_values(a_l_idx, m, false)?;
        let value_r = r.get_transitive_tiered_metric_values(a_r_idx, m, false)?;
        let delta = tg.get_transitive_tiered_delta(a_merged, m)?;

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
        "T2": 6.0,
        "T3": 5.0,
        "T4": 5.0,
    },
)
"#
        );

        let t_r_idx = r.data.node_names_ordered.name_to_idx_log("T").unwrap();
        let t_merged = tg.to_merged(GraphSide::Right, t_r_idx);
        let t_l_idx = tg.to_local(GraphSide::Left, t_merged);
        let value_l = match t_l_idx {
            Some(idx) => l.get_transitive_tiered_metric_values(idx, m, false)?,
            None => BTreeMap::new(),
        };
        let value_r = r.get_transitive_tiered_metric_values(t_r_idx, m, false)?;
        let delta = tg.get_transitive_tiered_delta(t_merged, m)?;

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

/// The delta skips nodes that didn't change, which is only sound because their
/// two sides cancel. A node whose tier moved does *not* cancel — it lands in a
/// different cumulative bucket on each side — so the tiered delta must stay
/// equal to `right - left` for every tier.
///
/// In this fixture the edge `B -> J` is tagged `RD` on the left and `RDFD` on
/// the right, which moves `J` from T3 to T2 without touching its metric.
#[cfg(test)]
mod tier_move_tests {
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph_with_tier_config;

    #[test]
    fn test_tier_moves_are_not_cancelled_out() -> Result<()> {
        let tg = make_twin_graph_with_tier_config()?;
        let nodes = ["A", "B", "D", "J", "T"];

        let mut rows = Vec::new();
        for name in nodes {
            rows.push((name, tiered_row(&tg, name)?));
        }

        snapshot!(
            format_table(&rows),
            "
node  tier         left      right      delta   right-left
A     T1              7          9          2            2
A     T2              9         15          6            6
A     T3             10         15          5            5
A     T4             11         16          5            5
B     T1              1          1          0            0
B     T2              2          6          4            4
B     T3              3          6          3            3
B     T4              4          7          3            3
D     T1              5          5          0            0
D     T2              7          7          0            0
D     T3              7          7          0            0
D     T4              7          7          0            0
J     T1              0          0          0            0
J     T2              1          5          4            4
J     T3              2          5          3            3
J     T4              2          5          3            3
T     T1              0          6          1            6
T     T2              0          8          1            8
T     T3              0          8          1            8
T     T4              0          8          1            8

"
        );
        Ok(())
    }

    /// `(tier, left, right, delta)` for every tier of `size` on one node.
    fn tiered_row(tg: &TwinGraph, name: &str) -> Result<Vec<(String, f64, f64, f64)>> {
        let m = "size";
        let r_idx = tg.r.data.node_names_ordered.name_to_idx_log(name).unwrap();
        let merged = tg.to_merged(GraphSide::Right, r_idx);
        let l_idx = tg.to_local(GraphSide::Left, merged);

        let left = match l_idx {
            Some(idx) => tg.l.get_transitive_tiered_metric_values(idx, m, false)?,
            None => BTreeMap::new(),
        };
        let right = tg.r.get_transitive_tiered_metric_values(r_idx, m, false)?;
        let delta = tg.get_transitive_tiered_delta(merged, m)?;

        Ok(tg
            .r
            .runtime
            .state
            .tiers
            .iter()
            .map(|(tier, _)| {
                let get = |map: &BTreeMap<String, f64>| *map.get(tier).unwrap_or(&0.0);
                (tier.clone(), get(&left), get(&right), get(&delta))
            })
            .collect())
    }

    fn format_table(rows: &[(&str, Vec<(String, f64, f64, f64)>)]) -> String {
        let mut out = format!(
            "{:<5} {:<8} {:>8} {:>10} {:>10} {:>12}\n",
            "node", "tier", "left", "right", "delta", "right-left"
        );
        for (name, tiers) in rows {
            for (tier, left, right, delta) in tiers {
                out.push_str(&format!(
                    "{:<5} {:<8} {:>8} {:>10} {:>10} {:>12}\n",
                    name,
                    tier,
                    left,
                    right,
                    delta,
                    right - left
                ));
            }
        }
        out
    }
}

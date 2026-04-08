// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::NodeIDX;
use crate::TwinGraph;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::Arrow;
use crate::types::twin_graph::GraphSide;
use crate::types::twin_graph::NodeDiff;

/// When working with twin graphs we want to get the list of
/// arrows for the node in both graphs and match them together.
///
/// If the arrow is present in both graphs, well return (Some<Arrow>, Some<Arrow>)
/// and both arrows will represent the same edge and they can possibly be slightly
/// different, e.g. the edge points to the same node but its tag changed.
///
/// If the edge exists in one graph but not the other we will return either
/// (Some<Arrow>, None) or (None, Some<Arrow>).
///
/// (None, None) is not a valid case.
pub(crate) fn get_twin_arrows(
    tg: &TwinGraph,
    merged_idx: NodeIDX,
    graph_structure: GraphStructure,
) -> Result<Vec<TwinArrow>> {
    let l = match tg.to_local(GraphSide::Left, merged_idx) {
        Some(local_idx) => tg.l.get_arrows(local_idx, graph_structure)?,
        None => vec![],
    };

    let r = match tg.to_local(GraphSide::Right, merged_idx) {
        Some(local_idx) => tg.r.get_arrows(local_idx, graph_structure)?,
        None => vec![],
    };

    merge_arrows(tg, l, r)
}

pub(crate) fn merge_arrows(
    tg: &TwinGraph,
    mut l: Vec<Arrow>,
    mut r: Vec<Arrow>,
) -> Result<Vec<TwinArrow>> {
    // Sort by target's merged IDX for consistent merge-join.
    let sort_key =
        |arrow: &Arrow, side: GraphSide| -> NodeIDX { tg.to_merged(side, arrow.points_to) };

    l.sort_by_key(|a| sort_key(a, GraphSide::Left));
    r.sort_by_key(|a| sort_key(a, GraphSide::Right));

    let mut l_iter = l.into_iter().peekable();
    let mut r_iter = r.into_iter().peekable();
    let mut result: Vec<TwinArrow> = Vec::new();

    loop {
        match (l_iter.peek(), r_iter.peek()) {
            (Some(l_arrow), Some(r_arrow)) => {
                let l_merged = tg.to_merged(GraphSide::Left, l_arrow.points_to);
                let r_merged = tg.to_merged(GraphSide::Right, r_arrow.points_to);

                match l_merged.cmp(&r_merged) {
                    std::cmp::Ordering::Less => {
                        let l_arrow = l_iter.next().unwrap();
                        let merged_from = tg.to_merged(GraphSide::Left, l_arrow.points_from);
                        result.push(TwinArrow {
                            points_to: l_merged,
                            points_from: merged_from,
                            node_diff: tg.node_diff[l_merged],
                            l: Some(l_arrow),
                            r: None,
                        });
                    }
                    std::cmp::Ordering::Greater => {
                        let r_arrow = r_iter.next().unwrap();
                        let merged_from = tg.to_merged(GraphSide::Right, r_arrow.points_from);
                        result.push(TwinArrow {
                            points_to: r_merged,
                            points_from: merged_from,
                            node_diff: tg.node_diff[r_merged],
                            l: None,
                            r: Some(r_arrow),
                        });
                    }
                    std::cmp::Ordering::Equal => {
                        let l_arrow = l_iter.next().unwrap();
                        let r_arrow = r_iter.next().unwrap();
                        let merged_from = tg.to_merged(GraphSide::Left, l_arrow.points_from);
                        result.push(TwinArrow {
                            points_to: l_merged,
                            points_from: merged_from,
                            node_diff: tg.node_diff[l_merged],
                            l: Some(l_arrow),
                            r: Some(r_arrow),
                        });
                    }
                }
            }
            (Some(_l_arrow), None) => {
                let l_arrow = l_iter.next().unwrap();
                let merged_to = tg.to_merged(GraphSide::Left, l_arrow.points_to);
                let merged_from = tg.to_merged(GraphSide::Left, l_arrow.points_from);
                result.push(TwinArrow {
                    points_to: merged_to,
                    points_from: merged_from,
                    node_diff: tg.node_diff[merged_to],
                    l: Some(l_arrow),
                    r: None,
                });
            }
            (None, Some(_r_arrow)) => {
                let r_arrow = r_iter.next().unwrap();
                let merged_to = tg.to_merged(GraphSide::Right, r_arrow.points_to);
                let merged_from = tg.to_merged(GraphSide::Right, r_arrow.points_from);
                result.push(TwinArrow {
                    points_to: merged_to,
                    points_from: merged_from,
                    node_diff: tg.node_diff[merged_to],
                    l: None,
                    r: Some(r_arrow),
                });
            }
            (None, None) => break,
        }
    }

    Ok(result)
}

/// Matched arrows pair represents either a single arrow if we have a single graph
/// or two optional arrows if we're comparing two graphs.
/// There should not be a situation where we have both arrows null.
///
/// `points_to` and `points_from` are in the merged (TwinGraph) namespace.
#[derive(serde::Serialize, typegen::TypeGen)]
pub struct TwinArrow {
    pub points_to: NodeIDX,
    pub points_from: NodeIDX,
    #[typegen(as = "u32")]
    pub node_diff: NodeDiff,
    pub l: Option<Arrow>,
    pub r: Option<Arrow>,
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_utils::print_twin_arrows;

    #[test]
    fn test_get_twin_arrows() -> Result<()> {
        let tg = make_twin_graph()?;

        let f_idx = tg.r.data.node_names_ordered.name_to_idx_log("F").unwrap();
        let f_merged = tg.to_merged(GraphSide::Right, f_idx);

        snapshot!(
            print_twin_arrows(
                &tg.r,
                &get_twin_arrows(&tg, f_merged, GraphStructure::Forward)?
            ),
            r#"
L: F -> G
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: F -> G
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

--------

L: F -> H
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: F -> H

--------

L: F -> I
   branch: b2
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: F -> I
   branch: b2
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}
"#
        );
        let j_idx = tg.r.data.node_names_ordered.name_to_idx_log("J").unwrap();
        let j_merged = tg.to_merged(GraphSide::Right, j_idx);

        snapshot!(
            print_twin_arrows(
                &tg.r,
                &get_twin_arrows(&tg, j_merged, GraphStructure::Forward)?
            ),
            "
L: J -> K

R: J -> K

--------

L:

R: J -> Q

--------

L:

R: J -> R

--------

L:

R: J -> S
"
        );
        let b_idx = tg.r.data.node_names_ordered.name_to_idx_log("B").unwrap();
        let b_merged = tg.to_merged(GraphSide::Right, b_idx);

        snapshot!(
            print_twin_arrows(
                &tg.r,
                &get_twin_arrows(&tg, b_merged, GraphStructure::Forward)?
            ),
            "
L: B -> C
   tag: BL

R: B -> C
   tag: BL

--------

L: B -> J
   tag: RD

R: B -> J
   tag: RDFD
"
        );

        let h_idx = tg.r.data.node_names_ordered.name_to_idx_log("H").unwrap();
        let h_merged = tg.to_merged(GraphSide::Right, h_idx);
        snapshot!(
            print_twin_arrows(
                &tg.r,
                &get_twin_arrows(&tg, h_merged, GraphStructure::Reverse)?
            ),
            r#"
L: H -> F
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: H -> F
"#
        );

        let q_idx = tg.r.data.node_names_ordered.name_to_idx_log("Q").unwrap();
        let q_merged = tg.to_merged(GraphSide::Right, q_idx);
        snapshot!(
            print_twin_arrows(
                &tg.r,
                &get_twin_arrows(&tg, q_merged, GraphStructure::Reverse)?
            ),
            "
L:

R: Q -> J
"
        );
        Ok(())
    }
}

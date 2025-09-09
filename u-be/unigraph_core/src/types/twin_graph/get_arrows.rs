// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::NodeIDX;
use crate::TwinGraph;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::Arrow;
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
    node_idx: NodeIDX,
    graph_structure: GraphStructure,
) -> Result<Vec<TwinArrow>> {
    let l = tg.l.get_arrows(node_idx, graph_structure)?;
    let r =
        tg.r.as_ref()
            .map(|r| r.get_arrows(node_idx, graph_structure))
            .transpose()?
            .unwrap_or_default();

    merge_arrows(tg, l, r)
}

pub(crate) fn merge_arrows(
    tg: &TwinGraph,
    mut l: Vec<Arrow>,
    mut r: Vec<Arrow>,
) -> Result<Vec<TwinArrow>> {
    l.sort_by(|a, b| a.points_to.cmp(&b.points_to));
    r.sort_by(|a, b| a.points_to.cmp(&b.points_to));

    let mut l_iter = l.into_iter().peekable();
    let mut r_iter = r.into_iter().peekable();
    let mut result: Vec<TwinArrow> = Vec::new();

    loop {
        match (l_iter.peek(), r_iter.peek()) {
            (Some(l_arrow), Some(r_arrow)) => {
                match l_arrow.points_to.cmp(&r_arrow.points_to) {
                    std::cmp::Ordering::Less => {
                        let points_to = l_arrow.points_to;

                        // l arrow has smaller points_to value
                        result.push(TwinArrow {
                            points_to,
                            points_from: l_arrow.points_from,
                            node_diff: tg.node_diff[points_to],
                            l: Some(l_iter.next().unwrap()),
                            r: None,
                        });
                    }
                    std::cmp::Ordering::Greater => {
                        let points_to = r_arrow.points_to;
                        // r arrow has smaller points_to value
                        result.push(TwinArrow {
                            points_to,
                            points_from: r_arrow.points_from,
                            node_diff: tg.node_diff[points_to],
                            l: None,
                            r: Some(r_iter.next().unwrap()),
                        });
                    }
                    std::cmp::Ordering::Equal => {
                        let points_to = r_arrow.points_to;
                        // Both arrows have the same points_to value
                        result.push(TwinArrow {
                            points_to,
                            points_from: l_arrow.points_from,
                            node_diff: tg.node_diff[points_to],
                            l: Some(l_iter.next().unwrap()),
                            r: Some(r_iter.next().unwrap()),
                        });
                    }
                }
            }
            (Some(l_arrow), None) => {
                let points_to = l_arrow.points_to;
                // Only l has remaining elements
                result.push(TwinArrow {
                    points_to,
                    points_from: l_arrow.points_from,
                    node_diff: tg.node_diff[points_to],
                    l: Some(l_iter.next().unwrap()),
                    r: None,
                });
            }
            (None, Some(r_arrow)) => {
                let points_to = r_arrow.points_to;
                // Only r has remaining elements
                result.push(TwinArrow {
                    points_to,
                    points_from: r_arrow.points_from,
                    node_diff: tg.node_diff[points_to],
                    l: None,
                    r: Some(r_iter.next().unwrap()),
                });
            }
            (None, None) => {
                // Both iterators are exhausted
                break;
            }
        }
    }

    Ok(result)
}

/// Matched arrows pair represents either a single arrow if we have a single graph
/// or two optional arrows if we're comparing two graphs.
/// There should not be a situation where we have both arrows null.
///
/// if we have two arrows they must BOTH point TO and FROM the same node
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

        let f_idx = tg.node_names.name_to_idx_log("F").unwrap();

        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, f_idx, GraphStructure::Forward)?
            ),
            r#"
L: F -> G
   branch: b1
   properties: {"type": "DDD"}

R: F -> G
   branch: b1
   properties: {"type": "DDD"}

--------

L: F -> H
   branch: b1
   properties: {"type": "DDD"}

R: F -> H

--------

L: F -> I
   branch: b2
   properties: {"type": "DDD"}

R: F -> I
   branch: b2
   properties: {"type": "DDD"}
"#
        );
        let j_idx = tg.node_names.name_to_idx_log("J").unwrap();

        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, j_idx, GraphStructure::Forward)?
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
        let b_idx = tg.node_names.name_to_idx_log("B").unwrap();

        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, b_idx, GraphStructure::Forward)?
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

        let h_idx = tg.node_names.name_to_idx_log("H").unwrap();
        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, h_idx, GraphStructure::Reverse)?
            ),
            r#"
L: H -> F
   branch: b1
   properties: {"type": "DDD"}

R: H -> F
"#
        );

        let q_idx = tg.node_names.name_to_idx_log("Q").unwrap();
        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, q_idx, GraphStructure::Reverse)?
            ),
            "
L:

R: Q -> J
"
        );
        Ok(())
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashSet;
use std::collections::VecDeque;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::GraphSide;
use crate::NodeIDX;
use crate::TwinGraph;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::Arrow;
use crate::types::array_graph::edge_to_arrow;
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

pub(crate) fn get_twin_arrows_changed_nodes_only(
    tg: &TwinGraph,
    node_idx: NodeIDX,
    graph_structure: GraphStructure,
) -> Result<Vec<TwinArrow>> {
    let right_graph = tg
        .graph(GraphSide::Right)
        .context("arrows: changed nodes only")?;

    let l = get_arrows_changed_nodes_only(
        &tg.node_diff,
        &tg.l,
        right_graph,
        node_idx,
        graph_structure,
        GraphSide::Left,
    )?;
    let r = get_arrows_changed_nodes_only(
        &tg.node_diff,
        &tg.l,
        right_graph,
        node_idx,
        graph_structure,
        GraphSide::Right,
    )?;

    #[cfg(test)]
    {
        use crate::tests::test_utils::print_arrow;
        for la in &l {
            eprintln!("Left: {}", print_arrow(&tg.l, la));
        }

        for ra in &r {
            eprintln!("Right: {}", print_arrow(right_graph, ra));
        }
    }

    merge_arrows(tg, l, r)
}

pub(crate) fn get_arrows_changed_nodes_only(
    node_diff: &[NodeDiff],
    left: &ArrayGraph,
    right: &ArrayGraph,
    node_idx: NodeIDX,
    graph_structure: GraphStructure,
    side: GraphSide,
) -> Result<Vec<Arrow>> {
    let target_graph = match side {
        GraphSide::Left => left,
        GraphSide::Right => right,
    };

    let offset_graph = match graph_structure {
        GraphStructure::Forward => &target_graph.edges_forward,
        GraphStructure::Reverse => &target_graph.derived_state.edges_reverse,
        GraphStructure::Dominator => target_graph.edges_dom(),
    };

    let mut visited: HashSet<NodeIDX> = HashSet::from([node_idx]);

    let mut queue = VecDeque::from([(node_idx, 0usize)]);
    let mut needles: Vec<Arrow> = Vec::new();

    // We're doing a BFS here from the root to changed nodes only. (and cut the traversal
    // when we hit a changed node).
    while let Some((current_node_idx, current_depth)) = queue.pop_front() {
        for (edge, metadata) in offset_graph.edges_with_metadata(current_node_idx) {
            let points_to = edge.points_to;
            let edges_changed = node_diff[points_to].has_changed_edgses();
            let metrics_changed = node_diff[points_to].has_changed_metrics();

            let left_unreachable = left.is_node_unreachable(points_to);
            let right_unreachable = right.is_node_unreachable(points_to);
            let node_changed =
                edges_changed || metrics_changed || left_unreachable != right_unreachable;

            // if it's a changed node we add the arrow for it and stop the traversal.
            // we don't want to go any further than that.
            if node_changed {
                let needle = if current_node_idx == node_idx {
                    // if it's a direct node we want to have the legit arrow with
                    // al the info about the edge.
                    edge_to_arrow(target_graph, current_node_idx, edge, metadata)?
                } else {
                    // if it's NOT a direct arrow and has some nodes in between our start
                    // and the needle then we don't really want to show all the edge info
                    // because this does not represent an actual edge in the graph.
                    Arrow {
                        tag: None,
                        branch: None,
                        properties: None,
                        points_from: node_idx,
                        points_to,
                        excluded: false,
                        message: None,
                    }
                };

                needles.push(needle);
            } else {
                // if it's not a changed node we continue the traversal
                if visited.insert(points_to) {
                    queue.push_back((points_to, current_depth + 1));
                }
            }
        }
    }

    Ok(needles)
}

fn merge_arrows(tg: &TwinGraph, mut l: Vec<Arrow>, mut r: Vec<Arrow>) -> Result<Vec<TwinArrow>> {
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
    use crate::ArrayGraph;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_utils::print_arrow;

    fn print_twin_arrows(ag: &ArrayGraph, twin_arrows: &Vec<TwinArrow>) -> String {
        let mut result = Vec::new();

        for twin_arrow in twin_arrows {
            let TwinArrow { l, r, .. } = twin_arrow;
            result.push(format!(
                "L: {:?}\n\nR: {:?}",
                l.as_ref().map(|a| print_arrow(ag, a)),
                r.as_ref().map(|a| print_arrow(ag, a))
            ));
        }
        result.join("\n\n--------\n\n").trim().to_string()
    }

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
L: Some("F -> G\
   branch: b1\
   properties: {\\"type\\": \\"DDD\\"}")

R: Some("F -> G\
   branch: b1\
   properties: {\\"type\\": \\"DDD\\"}")

--------

L: Some("F -> H\
   branch: b1\
   properties: {\\"type\\": \\"DDD\\"}")

R: Some("F -> H")

--------

L: Some("F -> I\
   branch: b2\
   properties: {\\"type\\": \\"DDD\\"}")

R: Some("F -> I\
   branch: b2\
   properties: {\\"type\\": \\"DDD\\"}")
"#
        );
        let j_idx = tg.node_names.name_to_idx_log("J").unwrap();

        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, j_idx, GraphStructure::Forward)?
            ),
            r#"
L: Some("J -> K")

R: Some("J -> K")

--------

L: None

R: Some("J -> Q")

--------

L: None

R: Some("J -> R")

--------

L: None

R: Some("J -> S")
"#
        );
        let b_idx = tg.node_names.name_to_idx_log("B").unwrap();

        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, b_idx, GraphStructure::Forward)?
            ),
            r#"
L: Some("B -> C\
   tag: BL")

R: Some("B -> C\
   tag: BL")

--------

L: Some("B -> J\
   tag: RD")

R: Some("B -> J\
   tag: RDFD")
"#
        );

        let h_idx = tg.node_names.name_to_idx_log("H").unwrap();
        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, h_idx, GraphStructure::Reverse)?
            ),
            r#"
L: Some("H -> F\
   branch: b1\
   properties: {\\"type\\": \\"DDD\\"}")

R: Some("H -> F")
"#
        );

        let q_idx = tg.node_names.name_to_idx_log("Q").unwrap();
        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows(&tg, q_idx, GraphStructure::Reverse)?
            ),
            r#"
L: None

R: Some("Q -> J")
"#
        );
        Ok(())
    }

    #[test]
    fn test_get_twin_arrows_changed_nodes_only() -> Result<()> {
        let tg = make_twin_graph()?;

        let a_idx = tg.node_names.name_to_idx_log("A").unwrap();

        snapshot!(
            print_twin_arrows(
                &tg.l,
                &get_twin_arrows_changed_nodes_only(&tg, a_idx, GraphStructure::Forward)?
            ),
            r#"
L: Some("A -> B")

R: Some("A -> B")

--------

L: Some("A -> F")

R: Some("A -> F")

--------

L: None

R: Some("A -> T")
"#
        );
        Ok(())
    }
}

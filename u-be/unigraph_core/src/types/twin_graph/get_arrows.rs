// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::NodeIDX;
use crate::TwinGraph;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::Arrow;

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
pub(crate) fn get_arrows_pairs(
    tg: &TwinGraph,
    node_idx: NodeIDX,
    graph_structure: GraphStructure,
) -> Result<Vec<(Option<Arrow>, Option<Arrow>)>> {
    let mut l = tg.l.get_arrows(node_idx, graph_structure)?;
    let mut r =
        tg.r.as_ref()
            .map(|r| r.get_arrows(node_idx, graph_structure))
            .transpose()?
            .unwrap_or_default();

    l.sort_by(|a, b| a.points_to.cmp(&b.points_to));
    r.sort_by(|a, b| a.points_to.cmp(&b.points_to));

    let mut l_iter = l.into_iter().peekable();
    let mut r_iter = r.into_iter().peekable();
    let mut result = Vec::new();

    loop {
        match (l_iter.peek(), r_iter.peek()) {
            (Some(l_arrow), Some(r_arrow)) => {
                match l_arrow.points_to.cmp(&r_arrow.points_to) {
                    std::cmp::Ordering::Less => {
                        // l arrow has smaller points_to value
                        result.push((Some(l_iter.next().unwrap()), None));
                    }
                    std::cmp::Ordering::Greater => {
                        // r arrow has smaller points_to value
                        result.push((None, Some(r_iter.next().unwrap())));
                    }
                    std::cmp::Ordering::Equal => {
                        // Both arrows have the same points_to value
                        result.push((Some(l_iter.next().unwrap()), Some(r_iter.next().unwrap())));
                    }
                }
            }
            (Some(_), None) => {
                // Only l has remaining elements
                result.push((Some(l_iter.next().unwrap()), None));
            }
            (None, Some(_)) => {
                // Only r has remaining elements
                result.push((None, Some(r_iter.next().unwrap())));
            }
            (None, None) => {
                // Both iterators are exhausted
                break;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::ArrayGraph;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_utils::print_arrow;

    fn print_matched_arrows(
        ag: &ArrayGraph,
        arrows: &Vec<(Option<Arrow>, Option<Arrow>)>,
    ) -> String {
        let mut result = Vec::new();

        for (l_arrow, r_arrow) in arrows {
            result.push(format!(
                "L: {:?}\n\nR: {:?}",
                l_arrow.as_ref().map(|a| print_arrow(ag, a)),
                r_arrow.as_ref().map(|a| print_arrow(ag, a))
            ));
        }
        result.join("\n\n--------\n\n").trim().to_string()
    }

    #[test]
    fn test_get_arrows_pairs() -> Result<()> {
        let tg = make_twin_graph()?;

        let f_idx = tg.node_names.name_to_idx_log("F").unwrap();

        snapshot!(
            print_matched_arrows(
                &tg.l,
                &get_arrows_pairs(&tg, f_idx, GraphStructure::Forward)?
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
            print_matched_arrows(
                &tg.l,
                &get_arrows_pairs(&tg, j_idx, GraphStructure::Forward)?
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
            print_matched_arrows(
                &tg.l,
                &get_arrows_pairs(&tg, b_idx, GraphStructure::Forward)?
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
            print_matched_arrows(
                &tg.l,
                &get_arrows_pairs(&tg, h_idx, GraphStructure::Reverse)?
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
            print_matched_arrows(
                &tg.l,
                &get_arrows_pairs(&tg, q_idx, GraphStructure::Reverse)?
            ),
            r#"
L: None

R: Some("Q -> J")
"#
        );
        Ok(())
    }
}

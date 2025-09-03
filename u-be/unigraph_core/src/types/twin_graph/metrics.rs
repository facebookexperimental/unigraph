// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;

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

    let should_count = |node_idx: &NodeIDX| {
        match (
            l.is_node_unreachable(*node_idx),
            r.is_node_unreachable(*node_idx),
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
    };

    let count_l = l
        .edges_forward
        .dfs_configured(&[node_idx])
        .filter(should_count)
        .count();

    let count_r = r
        .edges_forward
        .dfs_configured(&[node_idx])
        .filter(should_count)
        .count();

    Ok(count_r as i32 - count_l as i32)
}

#[cfg(test)]
mod tests {
    use k9::assert_equal;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;

    #[test]
    fn test_get_transitive_count_delta() -> Result<()> {
        let tg = make_twin_graph()?;
        let t_idx = tg.node_names.name_to_idx_log("T").unwrap();

        let t_left = tg.l.transitive_count_configured(t_idx);
        let t_right = tg.r.as_ref().unwrap().transitive_count_configured(t_idx);
        let t_delta = tg.get_transitive_count_delta(t_idx)?;

        assert_equal!(t_left, 1);
        assert_equal!(t_right, 8);
        assert_equal!(t_delta, 0);
        Ok(())
    }
}

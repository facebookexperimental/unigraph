use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::hash_map::Entry;

use crate::NodeIDX;
use crate::types::array_graph::offset_graph::OffsetGraph;

pub fn shortest_path_configured(
    offset_graph: &OffsetGraph,
    from: &[NodeIDX],
    to: NodeIDX,
) -> Option<Vec<NodeIDX>> {
    // here we will need to run a BFS on our directed graph in a very efficient way.

    // an edge case where roots contain the target node. Here we would want to return
    // that path of a single node right away. Otherwise the algorithm might start
    // setting parents for root nodes and then return a longer path than necessary.
    if from.contains(&to) {
        return Some(vec![to]);
    }

    // We can't put "current paths" on the stack for each entry cause it'll fragment
    // the memory, instead we'll be constructing a "reverse spanning tree" of parents
    // and then reconstruct the path from the "to" node to the "from" node.
    // This hashmap will also serve as a "visited" set, so we don't revisit nodes.
    let mut parents = HashMap::new();

    let mut queue = from.iter().copied().collect::<VecDeque<_>>();

    let mut needle = None;

    while let Some(current) = queue.pop_front() {
        if current == to {
            needle = Some(current);
            break;
        }

        // iterate over the edges of the current node
        for edge in offset_graph.edges_configured(current) {
            let child = edge.points_to;

            // if we have not seen this child before, add it to the queue
            if let Entry::Vacant(e) = parents.entry(child) {
                e.insert(current);
                queue.push_back(child);
            }
        }
    }

    // If we did find the target node, walk back through the parents
    // to reconstruct the path from the "to" node to the "from" node.
    // Then reverse the path to get it in the correct order.
    // If we didn't find the target node, return None.
    if let Some(needle) = needle {
        let mut path = Vec::new();
        let mut current = needle;

        while let Some(&parent) = parents.get(&current) {
            path.push(current);
            current = parent;
        }

        // push the starting node with no parent
        path.push(current);

        path.reverse();
        Some(path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::assert_equal;

    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::idx_to_names;
    use crate::tests::test_utils::name_to_idx;

    #[test]
    fn test_shortest_path_configured() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let p = ag
            .edges_forward
            .shortest_path_configured(&[name_to_idx(&ag, "A")], name_to_idx(&ag, "K"))
            .unwrap();

        assert_equal!(idx_to_names(&ag, p), vec!["A", "B", "J", "K"]);

        // FROM A NODE TO ITSELF
        let p = ag
            .edges_forward
            .shortest_path_configured(&[name_to_idx(&ag, "A")], name_to_idx(&ag, "A"))
            .unwrap();
        assert_equal!(idx_to_names(&ag, p), vec!["A"]);

        // FROM A NODE TO ITSELF IN A CYCLE
        let p = ag
            .edges_forward
            .shortest_path_configured(
                &[
                    name_to_idx(&ag, "M"),
                    name_to_idx(&ag, "N"),
                    name_to_idx(&ag, "O"),
                ],
                name_to_idx(&ag, "N"),
            )
            .unwrap();
        assert_equal!(idx_to_names(&ag, p), vec!["N"]);

        let p = ag
            .edges_forward
            .shortest_path_configured(
                &[name_to_idx(&ag, "A"), name_to_idx(&ag, "L")],
                name_to_idx(&ag, "H"),
            )
            .unwrap();

        assert_equal!(idx_to_names(&ag, p), vec!["A", "D", "F", "H"]);

        // REVERSE EDGES
        let p = ag
            .derived_state
            .edges_reverse
            .shortest_path_configured(
                &[name_to_idx(&ag, "F"), name_to_idx(&ag, "H")],
                name_to_idx(&ag, "A"),
            )
            .unwrap();

        assert_equal!(idx_to_names(&ag, p), vec!["H", "F", "D", "A"]);

        // NO PATHS
        let p = ag.derived_state.edges_reverse.shortest_path_configured(
            &[name_to_idx(&ag, "K"), name_to_idx(&ag, "E")],
            name_to_idx(&ag, "I"),
        );

        assert_equal!(p, None);

        Ok(())
    }
}

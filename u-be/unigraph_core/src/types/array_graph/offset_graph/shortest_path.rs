use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::hash_map::Entry;

use crate::NodeIDX;
use crate::types::array_graph::offset_graph::OffsetGraph;
use crate::types::array_graph::offset_graph::TraversalType;

pub fn shortest_path(
    offset_graph: &OffsetGraph,
    from: &[NodeIDX],
    to: NodeIDX,
    traversal_type: TraversalType,
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
        for edge in offset_graph.edges(current) {
            match traversal_type {
                TraversalType::Configured => {
                    // this is a bit annoying that we have to reach directly into
                    // excluded instead of using `edges_configured`, but it's a very
                    // hot loop and messing with dynamic objects will have some
                    // overhead
                    if edge.is_excluded() {
                        continue;
                    }
                }
                TraversalType::Unconfigured => {}
            }

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
            // NOTE: there is a capacity overflow error that happens sometimes with
            // the stack that looks like:
            //             hook.js:608 panicked at u-be/unigraph_core/src/types/array_graph/offset_graph/shortest_path.rs:74:18:
            // capacity overflow
            // Stack:
            // Error
            //     at imports.wbg.__wbg_new_8a6f238a6ece86ea (http://localhost:3000/?graph_settings=KLUv_QBodQUAAowiGIBtAwDCJ1H2OBbEHCX8K8A-TBmPJMyUBllVqfvY6X3_x_BZUv9b1yxqWGzHb7HfUx_kZ0102Gj2sWc1U9axL7ffttmqSm2-RK9rmBqk7sQOJMIBLMTP_lj_QlJ5fUNf2yjsF_OwXtz28q8IMp4abOwfUr1x_zprHTPPiQ1sqdSQ6svj__XFMZXKDCAQ5tg7qUxDMybiGFSaPhY3btMchoPVwiL4Cm6_IQNj:18091:31)
            //     at unigraph_wasm.wasm.__wbg_new_8a6f238a6ece86ea externref shim (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[5223]:0x2eb595)
            //     at unigraph_wasm.wasm.console_error_panic_hook::hook::hff5145660ffa5a5b (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[1997]:0x25e02e)
            //     at unigraph_wasm.wasm.core::ops::function::Fn::call::ha961929f72d9db20 (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[6149]:0x2eeaf9)
            //     at unigraph_wasm.wasm.std::panicking::rust_panic_with_hook::h645afae5b52932f1 (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[3102]:0x2b7cf8)
            //     at unigraph_wasm.wasm.std::panicking::begin_panic_handler::{{closure}}::h46b0698008fa5278 (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[3531]:0x2cd088)
            //     at unigraph_wasm.wasm.std::sys::backtrace::__rust_end_short_backtrace::h13efa606809930a8 (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[6048]:0x2ee731)
            //     at unigraph_wasm.wasm.__rustc[8abf7dcf45103d39]::rust_begin_unwind (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[4739]:0x2e7aa4)
            //     at unigraph_wasm.wasm.core::panicking::panic_fmt::h00d2fd22445b48f4 (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[4740]:0x2e7ad0)
            //     at unigraph_wasm.wasm.alloc::raw_vec::capacity_overflow::h8995664c64bcb3fb (wasm://wasm/unigraph_wasm.wasm-010137f2:wasm-function[4602]:0x2e5f71)
            //
            //
            // This happens rarely and will need to be debugged.
            // My guess there's some potential cycle or something. cause if BFS succeeds i don't see
            // how the path can exceed the number of BFS levels
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
    use crate::types::array_graph::offset_graph::TraversalType;

    #[test]
    fn test_shortest_path_configured() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let p = ag
            .edges_forward
            .shortest_path(
                &[name_to_idx(&ag, "A")],
                name_to_idx(&ag, "K"),
                TraversalType::Configured,
            )
            .unwrap();

        assert_equal!(idx_to_names(&ag, p), vec!["A", "B", "J", "K"]);

        // FROM A NODE TO ITSELF
        let p = ag
            .edges_forward
            .shortest_path(
                &[name_to_idx(&ag, "A")],
                name_to_idx(&ag, "A"),
                TraversalType::Configured,
            )
            .unwrap();
        assert_equal!(idx_to_names(&ag, p), vec!["A"]);

        // FROM A NODE TO ITSELF IN A CYCLE
        let p = ag
            .edges_forward
            .shortest_path(
                &[
                    name_to_idx(&ag, "M"),
                    name_to_idx(&ag, "N"),
                    name_to_idx(&ag, "O"),
                ],
                name_to_idx(&ag, "N"),
                TraversalType::Configured,
            )
            .unwrap();
        assert_equal!(idx_to_names(&ag, p), vec!["N"]);

        let p = ag
            .edges_forward
            .shortest_path(
                &[name_to_idx(&ag, "A"), name_to_idx(&ag, "L")],
                name_to_idx(&ag, "H"),
                TraversalType::Configured,
            )
            .unwrap();

        assert_equal!(idx_to_names(&ag, p), vec!["A", "D", "F", "H"]);

        // REVERSE EDGES
        let p = ag
            .derived_state
            .edges_reverse
            .shortest_path(
                &[name_to_idx(&ag, "F"), name_to_idx(&ag, "H")],
                name_to_idx(&ag, "A"),
                TraversalType::Configured,
            )
            .unwrap();

        assert_equal!(idx_to_names(&ag, p), vec!["H", "F", "D", "A"]);

        // NO PATHS
        let p = ag.derived_state.edges_reverse.shortest_path(
            &[name_to_idx(&ag, "K"), name_to_idx(&ag, "E")],
            name_to_idx(&ag, "I"),
            TraversalType::Configured,
        );

        assert_equal!(p, None);

        Ok(())
    }
}

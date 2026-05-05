// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::cmp::min;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::NodeIDX;
use crate::types::array_graph::offset_graph::DFSConfigured;
use crate::types::array_graph::offset_graph::OffsetGraph;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;

type TravIDX = usize;

/// A special value that indicates that a value is missing
const EMPTY_VALUE: TravIDX = usize::MAX;

/// https://en.wikipedia.org/wiki/Dominator_(graph_theory)
pub fn make_dominator_tree(
    targets: &[NodeIDX],
    flags: &[EdgeFlags],
    edge_offsets: &[usize],
    roots: &[NodeIDX],
) -> OffsetGraph {
    let node_count = edge_offsets.len() - 1;
    let mut current_trav_idx: TravIDX = 0;

    let mut dfs_stack: Vec<(NodeIDX, Option<usize>)> =
        roots.iter().map(|root| (*root, None)).collect::<Vec<_>>();

    let reachable_node_ct = DFSConfigured::new(targets, flags, edge_offsets, roots).count();

    // reverse graph (traversal order indexes)
    let mut reverse_graph: Vec<Vec<TravIDX>> = vec![vec![]; reachable_node_ct];
    // final dominator tree
    let mut dom_tree: Vec<BTreeSet<TravIDX>> = vec![BTreeSet::new(); reachable_node_ct];
    // reverse dominator tree
    let mut rev_dom_tree: Vec<BTreeSet<TravIDX>> = vec![BTreeSet::new(); reachable_node_ct];
    // For a node i, it stores a list of nodes for which i is the semi-dominator.
    // Initially it is empty.
    let mut bucket: Vec<Vec<TravIDX>> = vec![vec![]; reachable_node_ct];
    // map of nodes to their semi-dominators. initially maps each node to itself
    let mut semi_dom: Vec<TravIDX> = (0..reachable_node_ct).collect();
    // Map to immediate dominator of i’th node
    let mut dom: Vec<TravIDX> = (0..reachable_node_ct).collect();
    // parent of i’th node in the forest maintained during step 2 of the algorithm.
    let mut dsu: Vec<Option<TravIDX>> = vec![None; reachable_node_ct];
    //  At any point of time, label[i] stores the vertex v with minimum sdom, lying
    // on path from i to the root of the (dsu) tree in which node i lies. Initially, label[i]=i
    let mut label: Vec<TravIDX> = (0..reachable_node_ct).collect();
    //  mapping of i’th node to its new index, equal to the arrival time of node in dfs tree
    let mut node_idx_to_traversal_idx: Vec<TravIDX> = vec![EMPTY_VALUE; node_count];
    let mut traversal_idx_to_node_idx: Vec<NodeIDX> = vec![];
    // parent of node i in DFS tree. NOT NodeIDX, this is a TravIDX -> TravIDX map
    let mut dfs_trav_idx_parents: Vec<TravIDX> = (0..reachable_node_ct).collect();

    while let Some((node_idx, parent_trav_idx)) = dfs_stack.pop() {
        let mut trav_idx = node_idx_to_traversal_idx[node_idx];

        if trav_idx == EMPTY_VALUE {
            // this is the first time we see this node
            trav_idx = current_trav_idx;
            current_trav_idx += 1;

            node_idx_to_traversal_idx[node_idx] = trav_idx;
            traversal_idx_to_node_idx.push(node_idx);
            if let Some(parent_trav_idx) = parent_trav_idx {
                dfs_trav_idx_parents[trav_idx] = parent_trav_idx;
            }

            let start = edge_offsets[node_idx];
            let end = edge_offsets[node_idx + 1];
            for i in start..end {
                let target = targets[i];
                let flag = flags[i];
                if !flag.contains(EdgeFlags::EXCLUDED) {
                    dfs_stack.push((target, Some(trav_idx)));
                }
            }
        }

        if let Some(parent_trav_idx) = parent_trav_idx {
            reverse_graph[trav_idx].push(parent_trav_idx);
        }
    }

    for i in (0..current_trav_idx).rev() {
        for &child in &reverse_graph[i] {
            semi_dom[i] = min(
                semi_dom[i],
                semi_dom[dsu_find(&mut dsu, &semi_dom, &mut label, child)],
            );
        }

        if dfs_trav_idx_parents[i] != i {
            bucket[semi_dom[i]].push(i);
        }

        for &w in &bucket[i] {
            let v = dsu_find(&mut dsu, &semi_dom, &mut label, w);
            if semi_dom[v] == semi_dom[w] {
                dom[w] = semi_dom[w]
            } else {
                dom[w] = v;
            }
        }

        if dfs_trav_idx_parents[i] != i {
            dsu_union(&mut dsu, dfs_trav_idx_parents[i], i);
        }
    }

    for i in 0..current_trav_idx {
        if dfs_trav_idx_parents[i] != i {
            if dom[i] != semi_dom[i] {
                dom[i] = dom[dom[i]];
            }

            dom_tree[dom[i]].insert(i);
            rev_dom_tree[i].insert(dom[i]);
        }
    }

    let mut dom_targets = Vec::new();
    let mut dom_flags = Vec::new();
    let mut dom_edge_offsets = vec![0];

    for node in 0..node_count {
        let node_idx = NodeIDX::from(node);
        let trav_idx = node_idx_to_traversal_idx[node_idx];
        // If TRAV_IDX is missing that would mean that the node was not reachable
        // and is not a part of resulting DOM tree
        if trav_idx != EMPTY_VALUE {
            for &dom_child_trav_idx in &dom_tree[trav_idx] {
                let dom_child_node_idx = traversal_idx_to_node_idx[dom_child_trav_idx];
                dom_targets.push(dom_child_node_idx);
                dom_flags.push(EdgeFlags::empty());
            }
        }

        dom_edge_offsets.push(dom_targets.len());
    }

    OffsetGraph {
        targets: dom_targets,
        flags: dom_flags,
        edge_offsets: dom_edge_offsets,
        edge_metadata_map: BTreeMap::new(),
    }
}

fn dsu_find(
    dsu: &mut Vec<Option<TravIDX>>,
    sdom: &[TravIDX],
    label: &mut Vec<TravIDX>,
    u: TravIDX,
) -> TravIDX {
    if dsu[u].is_none() {
        u
    } else {
        dsu_compress(dsu, sdom, label, u)
    }
}

fn dsu_compress(
    dsu: &mut Vec<Option<TravIDX>>,
    sdom: &[TravIDX],
    label: &mut Vec<TravIDX>,
    u: TravIDX,
) -> TravIDX {
    let parent = dsu[u].expect("missing node in dsu");
    if dsu[parent].is_some() {
        let next = dsu_compress(dsu, sdom, label, parent);
        if sdom[next] < sdom[label[u]] {
            label[u] = next;
        }
        dsu[u] = dsu[parent];
    }

    *label.get(u).expect("missing node in labels")
}

fn dsu_union(dsu: &mut [Option<TravIDX>], id1: TravIDX, id2: TravIDX) {
    dsu[id2] = Some(id1);
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::assert_equal;
    use k9::snapshot;

    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::name_to_idx;

    #[test]
    fn test_dominator_tree() -> Result<()> {
        let ag = make_test_array_graph_2()?.append_super_root(false)?;

        snapshot!(
            ag.debug().to_dom_edges_string()?,
            r#"
A:
  - B
B:
  - J
  - C
C (labels: disallow_tags: [b, c]):
D:
  - E
E:
F:
  - I
  - H
  - G
G:
H:
I:
J (labels: assert_tags: [a, b]):
K:
L:
  - M
M:
  - O
N:
O:
  - P
  - N
P:
~root~:
  - L
  - F
  - D
  - K
  - A
"#
        );

        let a_idx = name_to_idx(&ag, "A");
        assert_equal!(ag.transitive_count_configured_dominated(a_idx), 4);

        let b_idx = name_to_idx(&ag, "B");
        assert_equal!(ag.transitive_count_configured_dominated(b_idx), 3);
        let c_idx = name_to_idx(&ag, "C");
        assert_equal!(ag.transitive_count_configured_dominated(c_idx), 1);
        Ok(())
    }
}

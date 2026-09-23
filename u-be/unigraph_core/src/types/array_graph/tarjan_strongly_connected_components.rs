// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;

#[allow(clippy::upper_case_acronyms)]
type SCCIDX = usize;

/// A special value that indicates that a value is missing
const MISSING: usize = usize::MAX;

pub struct SCCBuilder<'a> {
    array_graph: &'a ArrayGraph,
    node_on_stack: Vec<bool>,
    node_lowlink: Vec<SCCIDX>,
    stack: Vec<NodeIDX>,
    node_idx_to_scc_idx: Vec<SCCIDX>,
    curr_scc_idx: SCCIDX,
    result: Vec<Vec<NodeIDX>>,
}

impl<'a> SCCBuilder<'a> {
    pub fn new(array_graph: &'a ArrayGraph) -> Self {
        let node_count = array_graph.nodes_len();
        SCCBuilder {
            array_graph,
            node_on_stack: vec![false; node_count],
            node_lowlink: vec![0; node_count],
            stack: Vec::new(),
            result: Vec::new(),
            node_idx_to_scc_idx: vec![MISSING; node_count],
            curr_scc_idx: 0,
        }
    }

    pub fn build(mut self) -> Vec<Vec<NodeIDX>> {
        for node_idx in self.array_graph.node_idx_iter_reachable() {
            if self.node_idx_to_scc_idx[node_idx] == MISSING {
                self.strong_connect(node_idx);
            }
        }
        self.result
    }

    /// Tarjan's `strongconnect`, iteratively.
    ///
    /// The textbook form recurses once per DFS tree edge, so its depth is the
    /// DFS depth of the graph. At a few million nodes that is far past the
    /// 2 MiB stack a spawned thread gets, and the process takes a SIGSEGV
    /// rather than an error. `call_stack` holds each node's next un-examined
    /// edge, standing in for the caller's resume point.
    fn strong_connect(&mut self, root: NodeIDX) {
        self.open(root);
        let mut call_stack: Vec<(NodeIDX, usize)> = vec![(root, self.edge_start(root))];

        while let Some(&(node_idx, next_edge)) = call_stack.last() {
            if next_edge < self.edge_end(node_idx) {
                let top = call_stack.len() - 1;
                call_stack[top].1 += 1;
                if let Some(child) = self.follow(node_idx, next_edge) {
                    self.open(child);
                    call_stack.push((child, self.edge_start(child)));
                }
                continue;
            }

            // Out of edges: the node is finished. Emitting its component and
            // then folding its lowlink into its parent's is what the recursive
            // form did on the way out of the call.
            call_stack.pop();
            self.close(node_idx);
            if let Some(&(parent, _)) = call_stack.last() {
                self.node_lowlink[parent] =
                    self.node_lowlink[parent].min(self.node_lowlink[node_idx]);
            }
        }
    }

    /// First visit: number the node and put it on the component stack.
    fn open(&mut self, node_idx: NodeIDX) {
        self.node_lowlink[node_idx] = self.curr_scc_idx;
        self.node_idx_to_scc_idx[node_idx] = self.curr_scc_idx;
        self.curr_scc_idx += 1;
        self.stack.push(node_idx);
        self.node_on_stack[node_idx] = true;
    }

    fn edge_start(&self, node_idx: NodeIDX) -> usize {
        self.array_graph.data.edges.edge_offsets[node_idx]
    }

    fn edge_end(&self, node_idx: NodeIDX) -> usize {
        self.array_graph.data.edges.edge_offsets[node_idx + 1]
    }

    /// Examine one forward edge. Returns the child to descend into, or `None`
    /// when the edge is excluded or its target is already numbered — folding
    /// the target's index into the lowlink in the latter case.
    fn follow(&mut self, node_idx: NodeIDX, edge: usize) -> Option<NodeIDX> {
        if self.array_graph.runtime.edge_flags[edge].contains(EdgeFlags::EXCLUDED) {
            return None;
        }

        let points_to_node_idx = self.array_graph.data.edges.edges[edge];
        if self.node_idx_to_scc_idx[points_to_node_idx] == MISSING {
            return Some(points_to_node_idx);
        }

        if self.node_on_stack[points_to_node_idx] {
            let node_lowlink = self.node_lowlink[node_idx];
            let edge_scc_idx = self.node_idx_to_scc_idx[points_to_node_idx];
            self.node_lowlink[node_idx] = node_lowlink.min(edge_scc_idx);
        }
        None
    }

    /// A node whose lowlink never escaped its own index roots a component: pop
    /// everything above it off the stack.
    fn close(&mut self, node_idx: NodeIDX) {
        if self.node_lowlink[node_idx] != self.node_idx_to_scc_idx[node_idx] {
            return;
        }

        let mut scc = Vec::new();
        while let Some(top) = self.stack.pop() {
            self.node_on_stack[top] = false;
            scc.push(top);
            if top == node_idx {
                break;
            }
        }
        for &n in &scc {
            self.node_idx_to_scc_idx[n] = scc.len();
        }
        self.result.push(scc);
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_test_array_graph_2;

    #[test]
    fn test_sccs() -> Result<()> {
        let ag = make_test_array_graph_2()?;
        let sccs = SCCBuilder::new(&ag).build();

        let mut result = String::new();
        for scc in sccs.iter().peekable() {
            result.push('[');
            let mut iter = scc.iter().peekable();
            while let Some(node_idx) = iter.next() {
                let name = ag.idx_to_name(*node_idx);
                result.push_str(name);
                if iter.peek().is_some() {
                    result.push_str(", ");
                }
            }
            result.push_str("]\n");
        }

        snapshot!(
            result,
            "
[C]
[K]
[J]
[B]
[G]
[H]
[I]
[F]
[E]
[D]
[A]
[P]
[N, O, M]
[L]

"
        );
        Ok(())
    }
}

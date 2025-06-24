// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::ArrayGraph;
use crate::NodeIDX;

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

    fn strong_connect(&mut self, node_idx: NodeIDX) {
        self.node_lowlink[node_idx] = self.curr_scc_idx;
        self.node_idx_to_scc_idx[node_idx] = self.curr_scc_idx;
        self.curr_scc_idx += 1;
        self.stack.push(node_idx);
        self.node_on_stack[node_idx] = true;

        for edge in self.array_graph.edges_forward.edges_configured(node_idx) {
            let points_to_node_idx = edge.points_to;
            if self.node_idx_to_scc_idx[points_to_node_idx] == MISSING {
                self.strong_connect(points_to_node_idx);

                let node_lowlink = self.node_lowlink[node_idx];
                let edge_lowlink = self.node_lowlink[points_to_node_idx];

                self.node_lowlink[node_idx] = node_lowlink.min(edge_lowlink);
            } else if self.node_on_stack[points_to_node_idx] {
                let node_lowlink = self.node_lowlink[node_idx];
                let edge_scc_idx = self.node_idx_to_scc_idx[points_to_node_idx];
                self.node_lowlink[node_idx] = node_lowlink.min(edge_scc_idx);
            }
        }

        let node_lowlink = self.node_lowlink[node_idx];
        let node_scc_idx = self.node_idx_to_scc_idx[node_idx];

        if node_lowlink == node_scc_idx {
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

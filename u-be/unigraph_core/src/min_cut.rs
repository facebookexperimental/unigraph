// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Minimum edge cut over a directed [`ArrayGraph`].
//!
//! # The problem
//!
//! Given a set of *source* nodes (typically the graph's entry points) and a set
//! of *sink* nodes (a feature you want to delete), find the minimum set of
//! dependency edges to remove so that no sink is reachable from any source.
//! After removing those edges the whole feature — plus anything that was only
//! kept alive by it — becomes unreachable dead code.
//!
//! This is the classic **max-flow / min-cut** problem (Menger's theorem): the
//! minimum number of edges separating sources from sinks equals the maximum
//! flow between them. Dominator trees can't answer this once a target is
//! reachable via multiple independent paths — a cut can.
//!
//! # Construction
//!
//! We build a unit-capacity flow network:
//!
//! ```text
//!   SUPER_SOURCE --∞--> each source (entry point)
//!   a --1--> b           for every configured (non-excluded) graph edge
//!   each sink --∞--> SUPER_SINK
//! ```
//!
//! Real edges get capacity 1, so the max flow equals the minimum *number* of
//! edges to cut. The artificial source/sink edges get ∞ so the cut is forced
//! onto real dependency edges only.
//!
//! # Which cut
//!
//! There are usually many min cuts of equal size. We return the one **nearest
//! the sinks** (computed from the sink side of the residual graph): the "last
//! hop" import edges pointing into the feature, which are the edges you'd
//! actually delete.

use std::collections::BTreeSet;

use crate::NodeIDX;
use crate::types::array_graph::ArrayGraph;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;

/// Result of a [`min_cut`] computation.
pub struct MinCut {
    /// The edges to remove, as `(from, to)` node index pairs. Removing all of
    /// them makes every sink unreachable from every source. Empty if the sinks
    /// are already unreachable from the sources.
    pub cut_edges: Vec<(NodeIDX, NodeIDX)>,
    /// `true` when at least one sink coincides with a source (an entry point).
    /// Such a sink can never be made unreachable by cutting edges — you'd have
    /// to delete the module itself. When set, `cut_edges` reflects only the
    /// cuttable sinks.
    pub has_uncuttable_sink: bool,
    /// `true` when the sinks are reachable from the sources *only* through
    /// protected edges, so no cut avoiding them exists. When set, `cut_edges`
    /// is empty — the protection is too strict to sever the feature.
    pub blocked_by_protected: bool,
}

/// A single edge in a [`MinCutResult`], as node indices in the UI namespace.
#[derive(serde::Serialize, typegen::TypeGen)]
pub struct MinCutEdge {
    pub from: NodeIDX,
    pub to: NodeIDX,
}

/// Serialization-friendly form of [`MinCut`] for transport across the WASM
/// boundary. Tuples don't survive TypeGen cleanly, so `cut_edges` uses a named
/// [`MinCutEdge`] struct instead of `(NodeIDX, NodeIDX)`.
#[derive(serde::Serialize, typegen::TypeGen)]
pub struct MinCutResult {
    pub cut_edges: Vec<MinCutEdge>,
    pub has_uncuttable_sink: bool,
    pub blocked_by_protected: bool,
}

impl From<MinCut> for MinCutResult {
    fn from(cut: MinCut) -> Self {
        MinCutResult {
            cut_edges: cut
                .cut_edges
                .into_iter()
                .map(|(from, to)| MinCutEdge { from, to })
                .collect(),
            has_uncuttable_sink: cut.has_uncuttable_sink,
            blocked_by_protected: cut.blocked_by_protected,
        }
    }
}

/// Compute the minimum edge cut separating `sinks` from `sources`, never cutting
/// any edge in `protected`.
///
/// Operates purely in `NodeIDX` space. Callers resolve names to indices and
/// pass the graph's entry points as `sources`. Only configured (non-excluded)
/// edges are considered, matching the active traversal.
///
/// Protected edges are made uncuttable (infinite capacity), so the result is the
/// smallest cut that avoids all of them — always the same size or larger than
/// the unconstrained cut, never smaller. Pass an empty set for no protection.
pub fn min_cut(
    graph: &ArrayGraph,
    sources: &[NodeIDX],
    sinks: &[NodeIDX],
    protected: &BTreeSet<(NodeIDX, NodeIDX)>,
) -> MinCut {
    let uncuttable = sinks.iter().any(|s| sources.contains(s));
    let cuttable_sinks: Vec<NodeIDX> = sinks
        .iter()
        .copied()
        .filter(|s| !sources.contains(s))
        .collect();

    if sources.is_empty() || cuttable_sinks.is_empty() {
        return MinCut {
            cut_edges: Vec::new(),
            has_uncuttable_sink: uncuttable,
            blocked_by_protected: false,
        };
    }

    let mut network = FlowNetwork::build(graph, sources, &cuttable_sinks, protected);

    // If sinks are reachable from sources through infinite-capacity edges alone,
    // every cut must include a protected edge — there is no valid cut.
    if network.sinks_reachable_via_protected() {
        return MinCut {
            cut_edges: Vec::new(),
            has_uncuttable_sink: uncuttable,
            blocked_by_protected: true,
        };
    }

    network.max_flow();
    let cut_edges = network.extract_cut_nearest_sinks();

    MinCut {
        cut_edges,
        has_uncuttable_sink: uncuttable,
        blocked_by_protected: false,
    }
}

// -- Implementation -----------------------------------------------------------

/// Sentinel capacity for the artificial source/sink edges. The real max flow is
/// bounded by the number of unit-capacity graph edges, so this is never a
/// bottleneck.
const INF: i64 = i64::MAX / 4;

/// Dinic's max-flow over an adjacency-of-edge-ids representation. Forward and
/// backward edges are stored as consecutive pairs, so the reverse of edge `e`
/// is `e ^ 1`.
struct FlowNetwork {
    source: usize,
    sink: usize,
    /// `edge_to[e]` — target node of edge `e`.
    edge_to: Vec<u32>,
    /// `edge_cap[e]` — residual capacity of edge `e`.
    edge_cap: Vec<i64>,
    /// `adjacency[n]` — ids of edges leaving node `n`.
    adjacency: Vec<Vec<u32>>,
    /// Real graph edges we added, as `(from, to)`, in insertion order.
    real_edges: Vec<(NodeIDX, NodeIDX)>,
    level: Vec<i32>,
    iter_ptr: Vec<usize>,
}

impl FlowNetwork {
    fn build(
        graph: &ArrayGraph,
        sources: &[NodeIDX],
        sinks: &[NodeIDX],
        protected: &BTreeSet<(NodeIDX, NodeIDX)>,
    ) -> Self {
        let node_count = graph.nodes_len();
        let source = node_count;
        let sink = node_count + 1;
        let total = node_count + 2;

        let mut net = FlowNetwork {
            source,
            sink,
            edge_to: Vec::new(),
            edge_cap: Vec::new(),
            adjacency: vec![Vec::new(); total],
            real_edges: Vec::new(),
            level: vec![-1; total],
            iter_ptr: vec![0; total],
        };

        for &src in sources {
            net.add_edge(source, usize::from(src), INF);
        }
        for &sink_node in sinks {
            net.add_edge(usize::from(sink_node), sink, INF);
        }
        net.add_graph_edges(graph, protected);
        net
    }

    /// Add every configured (non-excluded) graph edge. Cuttable edges get unit
    /// capacity; protected edges get infinite capacity so the cut never picks
    /// them (and are excluded from `real_edges` since they can't be reported).
    /// Unreachable source nodes carry no flow (flow only enters at entry points
    /// and follows configured edges), so we skip them for memory.
    fn add_graph_edges(&mut self, graph: &ArrayGraph, protected: &BTreeSet<(NodeIDX, NodeIDX)>) {
        for from in graph.node_idx_iter() {
            if graph.is_node_unreachable(from) {
                continue;
            }
            for (to, flags) in graph.forward_edges(from) {
                if flags.contains(EdgeFlags::EXCLUDED) {
                    continue;
                }
                if protected.contains(&(from, to)) {
                    self.add_edge(usize::from(from), usize::from(to), INF);
                } else {
                    self.real_edges.push((from, to));
                    self.add_edge(usize::from(from), usize::from(to), 1);
                }
            }
        }
    }

    /// BFS from the source over infinite-capacity edges only. If it reaches the
    /// sink, the feature hangs off the sources purely through protected (and
    /// artificial) edges, so no cut can avoid the protected set. Run before
    /// `max_flow`, while capacities are pristine.
    fn sinks_reachable_via_protected(&self) -> bool {
        let mut visited = vec![false; self.adjacency.len()];
        let mut queue = std::collections::VecDeque::new();
        visited[self.source] = true;
        queue.push_back(self.source);
        while let Some(node) = queue.pop_front() {
            for &edge in &self.adjacency[node] {
                let next = self.edge_to[edge as usize] as usize;
                if self.edge_cap[edge as usize] >= INF && !visited[next] {
                    visited[next] = true;
                    queue.push_back(next);
                }
            }
        }
        visited[self.sink]
    }

    fn add_edge(&mut self, from: usize, to: usize, cap: i64) {
        let forward = self.edge_to.len() as u32;
        self.edge_to.push(to as u32);
        self.edge_cap.push(cap);
        self.adjacency[from].push(forward);

        let backward = self.edge_to.len() as u32;
        self.edge_to.push(from as u32);
        self.edge_cap.push(0);
        self.adjacency[to].push(backward);
    }

    fn max_flow(&mut self) {
        while self.build_levels() {
            self.iter_ptr.iter_mut().for_each(|p| *p = 0);
            self.push_blocking_flow();
        }
    }

    /// BFS from the source over edges with residual capacity, recording each
    /// node's distance layer. Returns whether the sink is still reachable.
    fn build_levels(&mut self) -> bool {
        self.level.iter_mut().for_each(|l| *l = -1);
        let mut queue = std::collections::VecDeque::new();
        self.level[self.source] = 0;
        queue.push_back(self.source);
        while let Some(node) = queue.pop_front() {
            for &edge in &self.adjacency[node] {
                let next = self.edge_to[edge as usize] as usize;
                if self.edge_cap[edge as usize] > 0 && self.level[next] < 0 {
                    self.level[next] = self.level[node] + 1;
                    queue.push_back(next);
                }
            }
        }
        self.level[self.sink] >= 0
    }

    /// Push blocking flow along the current level graph with an explicit stack.
    /// Never recurses — dependency graphs can be tens of thousands of levels
    /// deep, which would blow a recursive DFS's call stack. Advances per-node
    /// iterators so exhausted edges are never revisited, and prunes dead-end
    /// nodes out of the level graph.
    fn push_blocking_flow(&mut self) {
        // `nodes` is the current DFS path; `edges[i]` is the edge taken from
        // `nodes[i]` to `nodes[i + 1]`.
        let mut nodes = vec![self.source];
        let mut edges: Vec<usize> = Vec::new();

        while let Some(&node) = nodes.last() {
            if node == self.sink {
                self.augment_and_retreat(&mut nodes, &mut edges);
                continue;
            }
            match self.next_admissible_edge(node) {
                Some(edge) => {
                    nodes.push(self.edge_to[edge] as usize);
                    edges.push(edge);
                }
                None => {
                    // Dead end: drop it from the level graph and back up,
                    // stepping the parent past the edge that led here.
                    self.level[node] = -1;
                    nodes.pop();
                    if let Some(edge) = edges.pop() {
                        self.iter_ptr[self.edge_to[edge ^ 1] as usize] += 1;
                    }
                }
            }
        }
    }

    /// The next edge out of `node` with residual capacity that descends one
    /// level, or `None` if the node is exhausted. Advances the node's iterator
    /// past saturated/off-level edges as it goes.
    fn next_admissible_edge(&mut self, node: usize) -> Option<usize> {
        while self.iter_ptr[node] < self.adjacency[node].len() {
            let edge = self.adjacency[node][self.iter_ptr[node]] as usize;
            let next = self.edge_to[edge] as usize;
            if self.edge_cap[edge] > 0 && self.level[next] == self.level[node] + 1 {
                return Some(edge);
            }
            self.iter_ptr[node] += 1;
        }
        None
    }

    /// Reached the sink: augment along the current path by its bottleneck, then
    /// retreat to just before the first edge that saturated (earlier edges keep
    /// residual capacity and can carry more flow).
    fn augment_and_retreat(&mut self, nodes: &mut Vec<usize>, edges: &mut Vec<usize>) {
        let bottleneck = edges.iter().map(|&e| self.edge_cap[e]).min().unwrap_or(0);

        let mut first_saturated = edges.len();
        for (i, &edge) in edges.iter().enumerate() {
            self.edge_cap[edge] -= bottleneck;
            self.edge_cap[edge ^ 1] += bottleneck;
            if first_saturated == edges.len() && self.edge_cap[edge] == 0 {
                first_saturated = i;
            }
        }

        nodes.truncate(first_saturated + 1);
        edges.truncate(first_saturated);
    }

    /// Extract the min cut nearest the sinks: real edges crossing from the
    /// source side into the set of nodes that can still reach the sink in the
    /// residual graph. Such edges are guaranteed saturated by the min-cut
    /// theorem. Deduplicated on `(from, to)`.
    fn extract_cut_nearest_sinks(&self) -> Vec<(NodeIDX, NodeIDX)> {
        let can_reach_sink = self.nodes_reaching_sink();
        let mut cut = std::collections::BTreeSet::new();
        for &(from, to) in &self.real_edges {
            if !can_reach_sink[usize::from(from)] && can_reach_sink[usize::from(to)] {
                cut.insert((from, to));
            }
        }
        cut.into_iter().collect()
    }

    /// Reverse BFS from the sink over residual edges: a node can reach the sink
    /// if it has a residual edge to another node that can.
    fn nodes_reaching_sink(&self) -> Vec<bool> {
        let mut can_reach = vec![false; self.adjacency.len()];
        let mut queue = std::collections::VecDeque::new();
        can_reach[self.sink] = true;
        queue.push_back(self.sink);
        while let Some(node) = queue.pop_front() {
            for &edge in &self.adjacency[node] {
                let neighbor = self.edge_to[edge as usize] as usize;
                // The reverse of `edge` runs neighbor -> node; if it has
                // residual capacity, `neighbor` can reach the sink through here.
                if self.edge_cap[(edge ^ 1) as usize] > 0 && !can_reach[neighbor] {
                    can_reach[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        can_reach
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;
    use crate::GraphBuilder;

    /// Build a graph, run a min cut, and format the result as one line per case:
    /// `<sources> ⇢ <sinks> [!<protected>] => [<cut edges>] (uncuttable, blocked)`.
    fn run_case(
        edges: &[(&str, &str)],
        sources: &[&str],
        sinks: &[&str],
        protected: &[(&str, &str)],
    ) -> Result<String> {
        let mut builder = GraphBuilder::new();
        for &(from, to) in edges {
            builder.add_edge(from, to)?;
        }
        let graph = builder
            .build()
            .to_array_graph(&ll::Task::create_new("test"))?;

        let idx = |name: &str| graph.data.node_names_ordered.name_to_idx_log(name).unwrap();
        let source_idxs: Vec<NodeIDX> = sources.iter().map(|n| idx(n)).collect();
        let sink_idxs: Vec<NodeIDX> = sinks.iter().map(|n| idx(n)).collect();
        let protected_set: BTreeSet<(NodeIDX, NodeIDX)> =
            protected.iter().map(|&(f, t)| (idx(f), idx(t))).collect();

        let result = min_cut(&graph, &source_idxs, &sink_idxs, &protected_set);
        let cut: Vec<String> = result
            .cut_edges
            .iter()
            .map(|&(from, to)| format!("{}->{}", graph.idx_to_name(from), graph.idx_to_name(to)))
            .collect();

        let protect_note = if protected.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = protected
                .iter()
                .map(|&(f, t)| format!("{f}->{t}"))
                .collect();
            format!(" !{{{}}}", list.join(","))
        };

        Ok(format!(
            "{} ⇢ {}{} => [{}] (uncuttable={}, blocked={})",
            sources.join(","),
            sinks.join(","),
            protect_note,
            cut.join(", "),
            result.has_uncuttable_sink,
            result.blocked_by_protected,
        ))
    }

    #[test]
    fn min_cut_cases() -> Result<()> {
        let diamond = &[
            ("root", "m1"),
            ("root", "m2"),
            ("m1", "feat"),
            ("m2", "feat"),
            ("feat", "leaf"),
        ];
        let bottleneck = &[("root", "gate"), ("gate", "fa"), ("gate", "fb")];
        let out = [
            // Redundant diamond: feat reachable via m1 and m2. The min cut
            // nearest the sink severs both incoming edges of feat.
            run_case(diamond, &["root"], &["feat"], &[])?,
            // Protecting one incoming edge forces an equally small cut that
            // routes around it (root->m1 instead of the protected m1->feat).
            run_case(diamond, &["root"], &["feat"], &[("m1", "feat")])?,
            // Single bottleneck: the whole feature hangs off one edge, so one
            // cut suffices even though two feature nodes are reached.
            run_case(bottleneck, &["root"], &["fa", "fb"], &[])?,
            // Protecting the bottleneck pushes the cut down to the edges below.
            run_case(bottleneck, &["root"], &["fa", "fb"], &[("root", "gate")])?,
            // Protecting the only path to the feature makes the cut impossible.
            run_case(
                &[("root", "feat")],
                &["root"],
                &["feat"],
                &[("root", "feat")],
            )?,
            // A sink that is itself a source cannot be cut off — flagged, not cut.
            run_case(&[("root", "a"), ("a", "b")], &["root"], &["root"], &[])?,
            // A sink already unreachable from the sources needs no cut.
            run_case(
                &[("root", "a"), ("other", "orphan")],
                &["root"],
                &["orphan"],
                &[],
            )?,
        ];

        let table = out.join("\n");
        assert_eq!(
            table,
            "\
root ⇢ feat => [m1->feat, m2->feat] (uncuttable=false, blocked=false)
root ⇢ feat !{m1->feat} => [m2->feat, root->m1] (uncuttable=false, blocked=false)
root ⇢ fa,fb => [root->gate] (uncuttable=false, blocked=false)
root ⇢ fa,fb !{root->gate} => [gate->fa, gate->fb] (uncuttable=false, blocked=false)
root ⇢ feat !{root->feat} => [] (uncuttable=false, blocked=true)
root ⇢ root => [] (uncuttable=true, blocked=false)
root ⇢ orphan => [] (uncuttable=false, blocked=false)"
        );
        Ok(())
    }

    /// A single deep chain forces the flow DFS to descend the whole graph.
    /// The solver is fully iterative, so tens of thousands of levels must not
    /// overflow the stack (a recursive DFS would). The cut is the last hop.
    #[test]
    fn deep_chain_no_stack_overflow() -> Result<()> {
        const DEPTH: usize = 50_000;
        let name = |i: usize| format!("n{i:06}");

        let mut builder = GraphBuilder::new();
        for i in 0..DEPTH {
            builder.add_edge(&name(i), &name(i + 1))?;
        }
        let graph = builder
            .build()
            .to_array_graph(&ll::Task::create_new("test"))?;

        let idx = |n: &str| graph.data.node_names_ordered.name_to_idx_log(n).unwrap();
        let result = min_cut(
            &graph,
            &[idx(&name(0))],
            &[idx(&name(DEPTH))],
            &BTreeSet::new(),
        );

        let cut: Vec<(String, String)> = result
            .cut_edges
            .iter()
            .map(|&(from, to)| {
                (
                    graph.idx_to_name(from).to_owned(),
                    graph.idx_to_name(to).to_owned(),
                )
            })
            .collect();
        assert_eq!(cut, vec![(name(DEPTH - 1), name(DEPTH))]);
        Ok(())
    }
}

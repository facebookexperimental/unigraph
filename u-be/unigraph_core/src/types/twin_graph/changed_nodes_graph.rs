// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::VecDeque;
use std::sync::OnceLock;

use anyhow::Result;

use crate::Arrow;
use crate::EdgeMeta;
use crate::NodeIDX;
use crate::TraversalType;
use crate::TwinGraph;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::edge_to_arrow;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::EdgeGraphView;
use crate::types::array_graph::offset_graph::OffsetGraph;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::twin_graph::GraphSide;
use crate::types::twin_graph::get_arrows::TwinArrow;
use crate::types::twin_graph::get_arrows::merge_arrows;

pub struct ChangedNodesGraph {
    left: ChangedNodesGraphOneSide,
    right: ChangedNodesGraphOneSide,
}

impl ChangedNodesGraph {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            left: ChangedNodesGraphOneSide::new(),
            right: ChangedNodesGraphOneSide::new(),
        }
    }

    pub(crate) fn get_twin_arrows(
        &self,
        tg: &TwinGraph,
        merged_idx: NodeIDX,
        graph_structure: GraphStructure,
    ) -> Result<Vec<TwinArrow>> {
        let l = self
            .left
            .get_arrows(tg, GraphSide::Left, merged_idx, graph_structure)?;

        let r = self
            .right
            .get_arrows(tg, GraphSide::Right, merged_idx, graph_structure)?;

        merge_arrows(tg, l, r)
    }

    pub(crate) fn shortest_path(
        &self,
        tg: &TwinGraph,
        from: &[NodeIDX],
        to: NodeIDX,
        graph_structure: GraphStructure,
        traversal_type: TraversalType,
    ) -> Result<Option<Vec<NodeIDX>>> {
        let l = self.left.shortest_path(
            tg,
            from,
            to,
            GraphSide::Left,
            graph_structure,
            traversal_type,
        )?;

        let r = self.right.shortest_path(
            tg,
            from,
            to,
            GraphSide::Right,
            graph_structure,
            traversal_type,
        )?;

        match (l, r) {
            (Some(l_path), Some(r_path)) => {
                if l_path.len() <= r_path.len() {
                    Ok(Some(l_path))
                } else {
                    Ok(Some(r_path))
                }
            }
            (Some(l_path), None) => Ok(Some(l_path)),
            (None, Some(r_path)) => Ok(Some(r_path)),
            (None, None) => Ok(None),
        }
    }
}

struct ChangedNodesGraphOneSide {
    forward: OnceLock<ChangedNodesOffsetGraph>,
    reverse: OnceLock<ChangedNodesOffsetGraph>,
    dominator: OnceLock<ChangedNodesOffsetGraph>,
}

/// Additional wrapper over offset graph that also keeps track of
/// how many nodes were skipped to reach a certain node in the
/// original graph.
/// This graph operates in the MERGED index space.
struct ChangedNodesOffsetGraph {
    skipped: Vec<usize>,
    /// Per-edge metadata (None = Directed). Parallel to offset_graph.targets.
    edge_metadata: Vec<Option<EdgeMeta>>,
    offset_graph: OffsetGraph,
}

impl ChangedNodesGraphOneSide {
    #[allow(clippy::new_without_default)]
    fn new() -> Self {
        Self {
            forward: OnceLock::new(),
            reverse: OnceLock::new(),
            dominator: OnceLock::new(),
        }
    }

    fn get_arrows(
        &self,
        tg: &TwinGraph,
        side: GraphSide,
        merged_idx: NodeIDX,
        graph_structure: GraphStructure,
    ) -> Result<Vec<Arrow>> {
        log::info!("Getting changed-nodes arrows for {}", merged_idx);
        let graph = match graph_structure {
            GraphStructure::Forward => self.forward(tg, side)?,
            GraphStructure::Reverse => self.reverse(tg, side)?,
            GraphStructure::Dominator => self.dominator(tg, side)?,
        };
        log::info!("Got graph");

        let ag = tg.graph(side);
        let offset_graph = &graph.offset_graph;

        let mut result = Vec::new();

        // The changed-nodes graph uses merged IDXes for its CSR.
        let start = offset_graph.edge_offsets[merged_idx];
        let end = offset_graph.edge_offsets[merged_idx + 1];

        for edge_idx in start..end {
            let edge =
                Edge::new_with_flags(offset_graph.targets[edge_idx], offset_graph.flags[edge_idx]);
            let metadata = graph.edge_metadata[edge_idx].as_ref();
            let skipped = graph.skipped[edge_idx];

            // The edge points_to and points_from are in merged space.
            // Translate to local for edge_to_arrow, then the Arrow will use local IDXes.
            let local_from = tg.to_local(side, merged_idx);
            let local_to = tg.to_local(side, edge.points_to);

            if let (Some(from), Some(to)) = (local_from, local_to) {
                let local_edge = Edge {
                    points_to: to,
                    flags: edge.flags,
                };
                let mut arrow = edge_to_arrow(ag, from, local_edge, metadata)?;
                arrow.skipped = skipped;
                result.push(arrow);
            }
        }

        Ok(result)
    }

    pub(crate) fn shortest_path(
        &self,
        tg: &TwinGraph,
        from: &[NodeIDX],
        to: NodeIDX,
        side: GraphSide,
        graph_structure: GraphStructure,
        traversal_type: TraversalType,
    ) -> Result<Option<Vec<NodeIDX>>> {
        let offset_graph = match graph_structure {
            GraphStructure::Forward => &self.forward(tg, side)?.offset_graph,
            GraphStructure::Reverse => &self.reverse(tg, side)?.offset_graph,
            GraphStructure::Dominator => &self.dominator(tg, side)?.offset_graph,
        };

        // The changed-nodes graph operates in merged IDX space,
        // so from/to are already in the right space.
        Ok(offset_graph.shortest_path(from, to, traversal_type))
    }

    fn forward(&self, tg: &TwinGraph, side: GraphSide) -> Result<&ChangedNodesOffsetGraph> {
        log::info!("Building forward changed-nodes graph");
        self.forward
            .get_or_try_init(|| make_offset_graph(tg, side, GraphStructure::Forward))
    }

    fn reverse(&self, tg: &TwinGraph, side: GraphSide) -> Result<&ChangedNodesOffsetGraph> {
        log::info!("Building reverse changed-nodes graph");
        self.reverse
            .get_or_try_init(|| make_offset_graph(tg, side, GraphStructure::Reverse))
    }

    fn dominator(&self, tg: &TwinGraph, side: GraphSide) -> Result<&ChangedNodesOffsetGraph> {
        log::info!("Building dominator changed-nodes graph");
        self.dominator
            .get_or_try_init(|| make_offset_graph(tg, side, GraphStructure::Dominator))
    }
}

/// Build a changed-nodes offset graph in MERGED index space.
///
/// For every node `S` we want its "nearest changed descendants": the changed
/// nodes reachable from `S` via a path whose intermediate nodes are all
/// unchanged. Doing an independent forward BFS from each of the N nodes is
/// O(N·(V+E)) — it re-walks the same unchanged regions once per source and
/// freezes on large graphs.
///
/// Instead we run one **reverse** BFS from each *changed* node, walking
/// backwards through unchanged predecessors only. A node reached at reverse
/// depth `d` records the changed node as a descendant with `skipped = d`
/// (= number of unchanged intermediates). This computes the same frontier for
/// every source in ~O(Σ regions + E), and is naturally cycle-safe (BFS visited
/// = shortest path). Cost scales with the number of changed nodes, which is
/// exactly the common case (comparing two similar graphs).
///
/// Per-node edge *ordering* is irrelevant: `merge_arrows` re-sorts arrows by
/// target index and `shortest_path` only cares about connectivity, so only the
/// edge *set* (target, skipped, flags, metadata) must match the old build.
fn make_offset_graph(
    tg: &TwinGraph,
    side: GraphSide,
    graph_structure: GraphStructure,
) -> Result<ChangedNodesOffsetGraph> {
    let view = tg.graph(side).edge_view(graph_structure);

    let is_changed = compute_changed_flags(tg);
    let reverse_adjacency = build_reverse_adjacency(&view);
    let frontier = collect_changed_frontiers(tg, side, &reverse_adjacency, &is_changed);

    Ok(assemble_offset_graph(tg, side, &view, frontier))
}

/// Whether each merged node differs between the two sides (edges, metrics,
/// existence, or reachability). Indexed by merged IDX. A property of the merged
/// node, independent of which side we traverse.
fn compute_changed_flags(tg: &TwinGraph) -> Vec<bool> {
    tg.merged_node_idx_iter()
        .map(|merged_idx| is_node_changed(tg, merged_idx))
        .collect()
}

fn is_node_changed(tg: &TwinGraph, merged_idx: NodeIDX) -> bool {
    let diff = tg.node_diff[merged_idx];

    let existence_changed =
        diff.does_not_exist_in(GraphSide::Left) != diff.does_not_exist_in(GraphSide::Right);

    let reachability_changed = match (
        tg.to_local(GraphSide::Left, merged_idx)
            .map(|idx| tg.l.runtime.node_flags[idx].is_node_unreachable()),
        tg.to_local(GraphSide::Right, merged_idx)
            .map(|idx| tg.r.runtime.node_flags[idx].is_node_unreachable()),
    ) {
        (Some(l_unreach), Some(r_unreach)) => l_unreach != r_unreach,
        _ => false,
    };

    diff.has_changed_edgses()
        || diff.has_changed_metrics()
        || reachability_changed
        || existence_changed
}

/// Predecessor CSR (in LOCAL index space) over non-excluded edges of the given
/// structure view. `preds_of(n)` yields the local nodes with an edge into `n`.
struct ReverseAdjacency {
    offsets: Vec<usize>,
    preds: Vec<NodeIDX>,
}

impl ReverseAdjacency {
    fn preds_of(&self, node: NodeIDX) -> &[NodeIDX] {
        let n = usize::from(node);
        &self.preds[self.offsets[n]..self.offsets[n + 1]]
    }
}

fn build_reverse_adjacency(view: &EdgeGraphView<'_>) -> ReverseAdjacency {
    let node_count = view.node_count();

    let mut in_degrees = vec![0usize; node_count];
    for node in 0..node_count {
        for (target, _flags) in view.edges_configured(NodeIDX::from(node)) {
            in_degrees[usize::from(target)] += 1;
        }
    }

    let mut offsets = Vec::with_capacity(node_count + 1);
    offsets.push(0);
    for &degree in &in_degrees {
        offsets.push(offsets.last().unwrap() + degree);
    }

    let mut preds = vec![NodeIDX::from(0usize); offsets[node_count]];
    let mut cursor = offsets[..node_count].to_vec();
    for node in 0..node_count {
        for (target, _flags) in view.edges_configured(NodeIDX::from(node)) {
            let slot = &mut cursor[usize::from(target)];
            preds[*slot] = NodeIDX::from(node);
            *slot += 1;
        }
    }

    ReverseAdjacency { offsets, preds }
}

/// A changed descendant of some source node, in MERGED index space.
struct FrontierEdge {
    target: NodeIDX,
    /// Number of unchanged intermediate nodes between the source and `target`.
    skipped: usize,
}

/// For every merged node, its nearest changed descendants. Populated by one
/// reverse BFS per changed node.
fn collect_changed_frontiers(
    tg: &TwinGraph,
    side: GraphSide,
    reverse_adjacency: &ReverseAdjacency,
    is_changed: &[bool],
) -> Vec<Vec<FrontierEdge>> {
    let merged_len = tg.merged_len();
    let node_count = reverse_adjacency.offsets.len() - 1;

    let mut frontier: Vec<Vec<FrontierEdge>> = (0..merged_len).map(|_| Vec::new()).collect();

    // Epoch-stamped visited set, reused across BFS runs. `stamp[n] == epoch`
    // means `n` was visited in the current BFS; epochs start at 1 so 0 = unseen.
    let mut stamp = vec![0u32; node_count];
    let mut epoch: u32 = 0;
    let mut queue: VecDeque<(NodeIDX, usize)> = VecDeque::new();

    log::info!("Building changed-nodes graph ({side:?}): {merged_len} nodes");

    for target_merged in tg.merged_node_idx_iter() {
        if !is_changed[usize::from(target_merged)] {
            continue;
        }
        let Some(target_local) = tg.to_local(side, target_merged) else {
            continue;
        };

        epoch += 1;
        stamp[usize::from(target_local)] = epoch;
        queue.clear();
        queue.push_back((target_local, 0));

        while let Some((node_local, depth)) = queue.pop_front() {
            for &pred_local in reverse_adjacency.preds_of(node_local) {
                if stamp[usize::from(pred_local)] == epoch {
                    continue;
                }
                stamp[usize::from(pred_local)] = epoch;

                let pred_merged = tg.to_merged(side, pred_local);
                frontier[usize::from(pred_merged)].push(FrontierEdge {
                    target: target_merged,
                    skipped: depth,
                });

                // Only walk further back through unchanged nodes — a changed
                // node terminates the path (it is itself a frontier entry).
                if !is_changed[usize::from(pred_merged)] {
                    queue.push_back((pred_local, depth + 1));
                }
            }
        }
    }

    frontier
}

/// Flatten the per-node frontiers into the merged-space CSR. Direct edges
/// (`skipped == 0`) keep the original edge's flags + metadata; deeper edges are
/// plain directed edges.
fn assemble_offset_graph(
    tg: &TwinGraph,
    side: GraphSide,
    view: &EdgeGraphView<'_>,
    mut frontier: Vec<Vec<FrontierEdge>>,
) -> ChangedNodesOffsetGraph {
    let merged_len = tg.merged_len();

    let mut targets = Vec::new();
    let mut flags = Vec::new();
    let mut edge_metadata = Vec::new();
    let mut skipped = Vec::new();
    let mut edge_offsets = Vec::with_capacity(merged_len + 1);
    edge_offsets.push(0);

    for merged_idx in tg.merged_node_idx_iter() {
        for edge in std::mem::take(&mut frontier[usize::from(merged_idx)]) {
            let (edge_flags, metadata) = if edge.skipped == 0 {
                direct_edge_flags_and_meta(tg, side, view, merged_idx, edge.target)
            } else {
                (EdgeFlags::empty(), None)
            };
            targets.push(edge.target);
            flags.push(edge_flags);
            edge_metadata.push(metadata);
            skipped.push(edge.skipped);
        }
        edge_offsets.push(targets.len());
    }

    ChangedNodesOffsetGraph {
        skipped,
        edge_metadata,
        offset_graph: OffsetGraph {
            targets,
            flags,
            edge_offsets,
            edge_metadata_map: std::collections::BTreeMap::new(),
        },
    }
}

/// Flags + metadata of the first non-excluded edge `from -> to` in the structure
/// view (both args in MERGED space). Mirrors the original BFS, which preserved
/// the edge payload only for changed nodes that are *direct* children.
fn direct_edge_flags_and_meta(
    tg: &TwinGraph,
    side: GraphSide,
    view: &EdgeGraphView<'_>,
    from_merged: NodeIDX,
    to_merged: NodeIDX,
) -> (EdgeFlags, Option<EdgeMeta>) {
    let (Some(from_local), Some(to_local)) =
        (tg.to_local(side, from_merged), tg.to_local(side, to_merged))
    else {
        return (EdgeFlags::empty(), None);
    };

    for (edge, metadata) in view.edges_with_metadata(from_local) {
        if !edge.is_excluded() && edge.points_to == to_local {
            return (edge.flags, metadata.cloned());
        }
    }

    (EdgeFlags::empty(), None)
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_utils::print_twin_arrows;

    /// Characterization snapshot: dumps the full changed-nodes graph for every
    /// merged node across all three structures. Locks the edge set (targets,
    /// skipped counts, tags/branches) so the reverse-BFS build stays behavior
    /// identical to the original per-node BFS. Exercises cycles / multi-parent
    /// via `test_graph_2`.
    #[test]
    fn test_changed_nodes_graph_full_dump() -> Result<()> {
        let tg = make_twin_graph()?;
        snapshot!(
            dump_all_changed_nodes(&tg)?,
            r#"
═══ Forward ═══
── A ──
L: A -> B

R: A -> B

--------

L: A -> F
   skipped: 1

R: A -> F
   skipped: 1

--------

L:

R: A -> T
── B ──
L: B -> J
   tag: RD

R: B -> J
   tag: RDFD
── D ──
L: D -> F

R: D -> F
── J ──
L:

R: J -> Q

--------

L:

R: J -> R

--------

L:

R: J -> S
── L ──
L: L -> F
   skipped: 1

R: L -> F
   skipped: 1
── M ──
L: M -> F
   skipped: 1

R: M -> F
   skipped: 1
── N ──
L: N -> F
   skipped: 2

R: N -> F
   skipped: 2
── O ──
L: O -> F
   tag: BL

R: O -> F
   tag: BL
── T ──
L:

R: T -> F
   skipped: 1
── ~root~ ──
L: Q -> A

R: ~root~ -> A

--------

L: Q -> F
   skipped: 2

R: ~root~ -> F
   skipped: 2

═══ Reverse ═══
── B ──
L: B -> A

R: B -> A
── C ──
L: C -> B
   tag: BL

R: C -> B
   tag: BL
── D ──
L: D -> A

R: D -> A

--------

L:

R: D -> T
── E ──
L: E -> A
   skipped: 1

R: E -> A
   skipped: 1

--------

L:

R: E -> T
   skipped: 1
── F ──
L: F -> A
   skipped: 1

R: F -> A
   skipped: 1

--------

L:

R: F -> T
   skipped: 1
── G ──
L: G -> F
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: G -> F
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}
── H ──
L: H -> F
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: H -> F
── I ──
L: I -> F
   branch: b2
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}

R: I -> F
   branch: b2
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}
── J ──
L: J -> B
   tag: RD

R: J -> B
   tag: RDFD
── K ──
L: K -> A
   skipped: 2

R: K -> A
   skipped: 2

--------

L: K -> J

R: K -> J

--------

L:

R: K -> T
   skipped: 2
── Q ──
L:

R: Q -> J
── R ──
L:

R: R -> J
── S ──
L:

R: S -> J
── T ──
L:

R: T -> A

═══ Dominator ═══
── A ──
L: A -> B

R: A -> B

--------

L:

R: A -> T
── B ──
L: B -> J

R: B -> J
── J ──
L:

R: J -> Q

--------

L:

R: J -> R

--------

L:

R: J -> S
── ~root~ ──
L: Q -> A

R: ~root~ -> A

--------

L: Q -> F

R: ~root~ -> F
"#
        );
        Ok(())
    }

    fn dump_all_changed_nodes(tg: &TwinGraph) -> Result<String> {
        let mut sections = Vec::new();
        for structure in [
            GraphStructure::Forward,
            GraphStructure::Reverse,
            GraphStructure::Dominator,
        ] {
            let mut lines = vec![format!("═══ {structure:?} ═══")];
            for merged in tg.merged_node_idx_iter() {
                let arrows = tg.get_twin_arrows(merged, structure, true)?;
                if arrows.is_empty() {
                    continue;
                }
                lines.push(format!("── {} ──", tg.merged_idx_to_name(merged)));
                lines.push(print_twin_arrows(&tg.r, &arrows));
            }
            sections.push(lines.join("\n"));
        }
        Ok(sections.join("\n\n"))
    }

    #[test]
    fn test_get_twin_arrows_changed_nodes_only() -> Result<()> {
        let tg = make_twin_graph()?;

        let a_idx = tg.r.data.node_names_ordered.name_to_idx_log("A").unwrap();
        let a_merged = tg.to_merged(GraphSide::Right, a_idx);

        snapshot!(
            print_twin_arrows(
                &tg.r,
                &tg.get_twin_arrows(a_merged, GraphStructure::Forward, true)?
            ),
            "
L: A -> B

R: A -> B

--------

L: A -> F
   skipped: 1

R: A -> F
   skipped: 1

--------

L:

R: A -> T
"
        );
        Ok(())
    }
}

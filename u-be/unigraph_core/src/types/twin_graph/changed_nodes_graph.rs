// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashMap;
use std::collections::HashSet;
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
        let graph = match graph_structure {
            GraphStructure::Forward => self.forward(tg, side)?,
            GraphStructure::Reverse => self.reverse(tg, side)?,
            GraphStructure::Dominator => self.dominator(tg, side)?,
        };

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
        self.forward
            .get_or_try_init(|| make_offset_graph(tg, side, GraphStructure::Forward))
    }

    fn reverse(&self, tg: &TwinGraph, side: GraphSide) -> Result<&ChangedNodesOffsetGraph> {
        self.reverse
            .get_or_try_init(|| make_offset_graph(tg, side, GraphStructure::Reverse))
    }

    fn dominator(&self, tg: &TwinGraph, side: GraphSide) -> Result<&ChangedNodesOffsetGraph> {
        self.dominator
            .get_or_try_init(|| make_offset_graph(tg, side, GraphStructure::Dominator))
    }
}

/// Build a changed-nodes offset graph in MERGED index space.
/// Walks the local offset graph of the given side, translating IDXes to merged.
fn make_offset_graph(
    tg: &TwinGraph,
    side: GraphSide,
    graph_structure: GraphStructure,
) -> Result<ChangedNodesOffsetGraph> {
    let target_graph = tg.graph(side);

    let target_edge_view = target_graph.edge_view(graph_structure);

    let merged_len = tg.merged_len();

    let mut edges_map: HashMap<NodeIDX, Vec<(Edge, Option<EdgeMeta>, usize)>> = HashMap::new();

    for merged_idx in tg.merged_node_idx_iter() {
        let local_idx = tg.to_local(side, merged_idx);

        let closest_changed_children = match local_idx {
            Some(idx) => {
                get_edges_changed_nodes_only(tg, side, merged_idx, idx, &target_edge_view)?
            }
            None => vec![],
        };

        edges_map.insert(merged_idx, closest_changed_children);
    }

    let mut targets = vec![];
    let mut flags = vec![];
    let mut edge_metadata_vec = vec![];
    let mut skipped_vec = vec![];
    let mut edge_offsets = Vec::with_capacity(merged_len);

    edge_offsets.push(0);

    for merged_idx in tg.merged_node_idx_iter() {
        if let Some(children) = edges_map.remove(&merged_idx) {
            for (edge, metadata, skipped) in children {
                targets.push(edge.points_to);
                flags.push(edge.flags);
                edge_metadata_vec.push(metadata);
                skipped_vec.push(skipped);
            }
        }
        edge_offsets.push(targets.len());
    }

    Ok(ChangedNodesOffsetGraph {
        skipped: skipped_vec,
        edge_metadata: edge_metadata_vec,
        offset_graph: OffsetGraph {
            targets,
            flags,
            edge_offsets,
            edge_metadata_map: std::collections::BTreeMap::new(),
        },
    })
}

/// BFS from merged_idx through the local offset graph to find the nearest
/// changed nodes. Returns edges in MERGED index space.
fn get_edges_changed_nodes_only(
    tg: &TwinGraph,
    side: GraphSide,
    _merged_start: NodeIDX,
    local_start: NodeIDX,
    target_edge_view: &EdgeGraphView<'_>,
) -> Result<Vec<(Edge, Option<EdgeMeta>, usize)>> {
    // Visited set uses LOCAL IDXes (since we walk the local graph)
    let mut visited: HashSet<NodeIDX> = HashSet::from([]);

    let mut queue = VecDeque::from([(local_start, 0usize)]);
    let mut needles: Vec<(Edge, Option<EdgeMeta>, usize)> = Vec::new();

    while let Some((current_local_idx, current_depth)) = queue.pop_front() {
        if !visited.insert(current_local_idx) {
            continue;
        }

        for (edge, metadata) in target_edge_view.edges_with_metadata(current_local_idx) {
            let child_local = edge.points_to;

            if visited.contains(&child_local) {
                continue;
            }

            if edge.is_excluded() {
                continue;
            }

            // Translate child to merged IDX for diff lookup
            let child_merged = tg.to_merged(side, child_local);
            let child_diff = tg.node_diff[child_merged];

            let edges_changed = child_diff.has_changed_edgses();
            let metrics_changed = child_diff.has_changed_metrics();

            let left_existence = child_diff.does_not_exist_in(GraphSide::Left);
            let right_existence = child_diff.does_not_exist_in(GraphSide::Right);

            // Also check reachability change across sides
            let reachability_changed = if let (Some(l_unreach), Some(r_unreach)) = (
                tg.to_local(GraphSide::Left, child_merged)
                    .map(|idx| tg.l.runtime.node_flags[idx].is_node_unreachable()),
                tg.to_local(GraphSide::Right, child_merged)
                    .map(|idx| tg.r.runtime.node_flags[idx].is_node_unreachable()),
            ) {
                l_unreach != r_unreach
            } else {
                false
            };

            let node_changed = edges_changed
                || metrics_changed
                || reachability_changed
                || (left_existence != right_existence);

            if node_changed {
                // Build edge in MERGED space
                let merged_target = child_merged;
                let needle = if current_local_idx == local_start {
                    // Direct child: preserve edge metadata
                    let merged_edge = Edge {
                        points_to: merged_target,
                        flags: edge.flags,
                    };
                    (merged_edge, metadata.cloned(), current_depth)
                } else {
                    // Intermediate: plain directed edge
                    let merged_edge = Edge {
                        points_to: merged_target,
                        flags: EdgeFlags::empty(),
                    };
                    (merged_edge, None, current_depth)
                };

                needles.push(needle);
                visited.insert(child_local);
            } else {
                if !visited.contains(&child_local) {
                    queue.push_back((child_local, current_depth + 1));
                }
            }
        }
    }

    Ok(needles)
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_utils::print_twin_arrows;

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

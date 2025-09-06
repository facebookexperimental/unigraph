// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::OnceLock;

use anyhow::Result;

use crate::ArrayGraph;
use crate::Arrow;
use crate::GraphSide;
use crate::NodeIDX;
use crate::TwinGraph;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::edge_to_arrow;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;
use crate::types::array_graph::offset_graph::OffsetGraph;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::twin_graph::NodeDiff;

pub struct ChangedNodesGraph {
    forward: OnceLock<ChangedNodesOffsetGraph>,
    reverse: OnceLock<ChangedNodesOffsetGraph>,
    dominator: OnceLock<ChangedNodesOffsetGraph>,
}

/// Additional wraper over offset graph that also keeps track of
/// how many nodes were skipped to reach a certain node in the
/// original graph.
struct ChangedNodesOffsetGraph {
    skipped: Vec<usize>,
    offset_graph: OffsetGraph,
}

impl ChangedNodesGraph {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            forward: OnceLock::new(),
            reverse: OnceLock::new(),
            dominator: OnceLock::new(),
        }
    }

    pub fn get_arrows(
        &self,
        tg: &TwinGraph,
        side: GraphSide,
        node_idx: NodeIDX,
        graph_structure: GraphStructure,
    ) -> Result<Vec<Arrow>> {
        let graph = match graph_structure {
            GraphStructure::Forward => self.forward(tg, side)?,
            GraphStructure::Reverse => self.reverse(tg, side)?,
            GraphStructure::Dominator => self.dominator(tg, side)?,
        };

        let ag = match side {
            GraphSide::Left => &tg.l,
            GraphSide::Right => tg.graph(GraphSide::Right)?,
        };

        let offset_graph = &graph.offset_graph;

        let mut result = Vec::new();

        let start = offset_graph.edge_offsets[node_idx];
        let end = offset_graph.edge_offsets[node_idx + 1];

        for edge_idx in start..end {
            let edge = offset_graph.edges[edge_idx];
            let metadata = &offset_graph.non_directed_edges_metadata[edge_idx];
            let skipped = graph.skipped[edge_idx];

            let mut arrow = edge_to_arrow(ag, node_idx, edge, metadata)?;
            arrow.skipped = skipped;
            result.push(arrow);
        }

        Ok(result)
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

fn make_offset_graph(
    tg: &TwinGraph,
    side: GraphSide,
    graph_structure: GraphStructure,
) -> Result<ChangedNodesOffsetGraph> {
    let target_graph = tg.graph(side)?;

    let target_offset_graph = match graph_structure {
        GraphStructure::Forward => &target_graph.edges_forward,
        GraphStructure::Reverse => &target_graph.derived_state.edges_reverse,
        GraphStructure::Dominator => target_graph.edges_dom(),
    };

    let left_g = &tg.l;
    let right_g = tg.graph(GraphSide::Right)?;

    let mut stack = target_graph.determine_entrypoints();
    let mut visited: HashSet<NodeIDX> = HashSet::new();
    let mut edges_map: HashMap<NodeIDX, Vec<(Edge, NonDirectedEdgeMetadata, usize)>> =
        HashMap::new();

    while let Some(current_node_idx) = stack.pop() {
        if !visited.insert(current_node_idx) {
            continue;
        }

        let closest_changed_children = get_edges_changed_nodes_only(
            &tg.node_diff,
            left_g,
            right_g,
            current_node_idx,
            target_offset_graph,
        )?;

        for (edge, _metadata, _skipped) in &closest_changed_children {
            if visited.contains(&edge.points_to) {
                continue;
            }
            stack.push(edge.points_to);
        }
        edges_map.insert(current_node_idx, closest_changed_children);
    }

    let mut edges = vec![];
    let mut non_directed_edges_metadata = vec![];
    let mut skipped_vec = vec![];
    let mut edge_offsets = Vec::with_capacity(target_offset_graph.node_count());

    edge_offsets.push(0);

    for node_idx in tg.node_names.combined_node_idx_iter() {
        if let Some(children) = edges_map.remove(&node_idx) {
            for (edge, metadata, skipped) in children {
                edges.push(edge);
                non_directed_edges_metadata.push(metadata);
                skipped_vec.push(skipped);
            }
        }
        edge_offsets.push(edges.len());
    }

    Ok(ChangedNodesOffsetGraph {
        skipped: skipped_vec,
        offset_graph: OffsetGraph {
            edges,
            edge_offsets,
            non_directed_edges_metadata,
        },
    })
}

pub fn get_edges_changed_nodes_only(
    node_diff: &[NodeDiff],
    left: &ArrayGraph,
    right: &ArrayGraph,
    node_idx: NodeIDX,
    target_offset_graph: &OffsetGraph,
) -> Result<Vec<(Edge, NonDirectedEdgeMetadata, usize)>> {
    let mut visited: HashSet<NodeIDX> = HashSet::from([]);

    let mut queue = VecDeque::from([(node_idx, 0usize)]);
    let mut needles: Vec<(Edge, NonDirectedEdgeMetadata, usize)> = Vec::new();

    // We're doing a BFS here from the root to changed nodes only. (and cut the traversal
    // when we hit a changed node).
    while let Some((current_node_idx, current_depth)) = queue.pop_front() {
        if !visited.insert(current_node_idx) {
            continue;
        }

        for (edge, metadata) in target_offset_graph.edges_with_metadata(current_node_idx) {
            let points_to = edge.points_to;

            if visited.contains(&points_to) {
                continue;
            }

            if edge.is_excluded() {
                // We look for changed nodes on configured graph.
                // We could do it on unconfigured but it can be a much much
                // heavier operation and it won't bring much value.
                continue;
            }

            let edges_changed = node_diff[points_to].has_changed_edgses();
            let metrics_changed = node_diff[points_to].has_changed_metrics();

            let left_unreachable = left.node_flags[points_to].is_node_unreachable();
            let right_unreachable = right.node_flags[points_to].is_node_unreachable();

            let left_existence = node_diff[points_to].does_not_exist_in(GraphSide::Left);
            let right_existence = node_diff[points_to].does_not_exist_in(GraphSide::Right);

            let node_changed = edges_changed
                || metrics_changed
                || (left_unreachable != right_unreachable)
                || (left_existence != right_existence);

            // if it's a changed node we add the arrow for it and stop the traversal.
            // we don't want to go any further than that.
            if node_changed {
                let needle = if current_node_idx == node_idx {
                    // if it's a direct node we want to have the legit arrow with
                    // al the info about the edge.
                    (edge, metadata.clone(), current_depth)
                } else {
                    let edge = Edge {
                        points_to: edge.points_to,
                        flags: EdgeFlags::empty(),
                    };
                    // if it's NOT a direct arrow and has some nodes in between our start
                    // and the needle then we don't really want to show all the edge info
                    // because this does not represent an actual edge in the graph.
                    (edge, NonDirectedEdgeMetadata::Directed, current_depth)
                };

                needles.push(needle);

                // cut the traversal if we found a changed node
                visited.insert(points_to);
            } else {
                // if it's not a changed node we continue the traversal
                if !visited.contains(&points_to) {
                    queue.push_back((points_to, current_depth + 1));
                }
            }
        }
    }

    Ok(needles)
}

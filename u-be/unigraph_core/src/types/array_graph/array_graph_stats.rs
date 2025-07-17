// Copyright (c) Meta Platforms, Inc. and affiliates.

use ts_rs::TS;

use crate::ArrayGraph;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;

#[derive(serde::Serialize, TS)]
#[ts(export)]
pub struct ArrayGraphStats {
    pub num_all_nodes: usize,
    pub num_all_edges: usize,
    pub num_directed_edges: usize,
    pub num_tagged_edges: usize,
    pub num_dynamic_edges: usize,
    pub num_unreachable_nodes: usize,
    pub num_excluded_edges: usize,
    pub tier_names: Vec<String>,
}

impl ArrayGraphStats {
    pub fn from_array_graph(array_graph: &ArrayGraph) -> Self {
        let num_all_nodes = array_graph.nodes_len();
        let num_all_edges = array_graph.edges_forward.edges_len();

        let mut num_directed_edges = 0;
        let mut num_tagged_edges = 0;
        let mut num_dynamic_edges = 0;
        let mut num_excluded_edges = 0;

        for (_from, edge, metadata) in array_graph.edges_forward.iter_edges() {
            if edge.is_excluded() {
                num_excluded_edges += 1;
            }
            match metadata {
                NonDirectedEdgeMetadata::Directed => num_directed_edges += 1,
                NonDirectedEdgeMetadata::Tagged { .. } => num_tagged_edges += 1,
                NonDirectedEdgeMetadata::Dynamic { .. } => num_dynamic_edges += 1,
            }
        }

        let mut num_unreachable_nodes = 0;
        for node_idx in array_graph.node_idx_iter() {
            if array_graph.node_flags[node_idx].is_node_unreachable() {
                num_unreachable_nodes += 1;
            }
        }

        let tier_names = array_graph
            .state
            .tiers
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();

        Self {
            num_all_nodes,
            num_all_edges,
            num_directed_edges,
            num_tagged_edges,
            num_dynamic_edges,
            num_excluded_edges,
            num_unreachable_nodes,
            tier_names,
        }
    }
}

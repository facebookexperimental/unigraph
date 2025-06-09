// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::ArrayGraph;
use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::EdgeFlags;

pub fn idx_to_names<I: IntoIterator<Item = NodeIDX>>(graph: &ArrayGraph, idxs: I) -> Vec<String> {
    idxs.into_iter()
        .map(|idx| graph.idx_to_name(idx).to_string())
        .collect()
}

pub fn print_all_node_names(graph: &ArrayGraph) -> String {
    graph
        .node_names_ordered
        .iter_names()
        .map(|name| name.to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

pub fn print_edges(array_graph: &ArrayGraph) -> String {
    let mut result = String::new();
    for node_idx in array_graph.edges_forward.node_idx_iter() {
        let edges = array_graph.edges_forward.edges(node_idx);
        let from_name = array_graph.idx_to_name(node_idx);
        for edge in edges {
            let to_name = array_graph.idx_to_name(edge.points_to);
            let edge_type = if edge.flags.contains(EdgeFlags::IS_TAGGED) {
                "[T]"
            } else if edge.flags.contains(EdgeFlags::IS_DYNAMIC) {
                "[D]"
            } else {
                ""
            };
            let line = format!("{from_name} -> {to_name} {edge_type}");
            result.push_str(line.trim());
            result.push('\n');
        }
    }
    result.trim().to_string()
}

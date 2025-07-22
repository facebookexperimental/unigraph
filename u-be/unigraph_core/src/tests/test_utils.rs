// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod array_graph_test_trait;
pub mod traversal_config_test_trait;

use crate::ArrayGraph;
use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::edge_flags::EdgeType;

pub fn idx_to_names<I: IntoIterator<Item = NodeIDX>>(graph: &ArrayGraph, idxs: I) -> Vec<String> {
    idxs.into_iter()
        .map(|idx| graph.idx_to_name(idx).to_string())
        .collect()
}

pub fn name_to_idx(graph: &ArrayGraph, name: &str) -> NodeIDX {
    graph.node_names_ordered.name_to_idx_log(name).unwrap()
}

pub fn print_all_node_names(graph: &ArrayGraph) -> String {
    graph
        .node_names_ordered
        .iter_names()
        .map(|name| name.to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

pub fn print_forward_edges(array_graph: &ArrayGraph) -> String {
    print_edges(array_graph, |graph, node_idx| {
        graph.edges_forward.edges(node_idx)
    })
}

pub fn print_edges<FN>(array_graph: &ArrayGraph, edges_fn: FN) -> String
where
    FN: Fn(&ArrayGraph, NodeIDX) -> &[Edge],
{
    let mut result = String::new();
    for node_idx in array_graph.edges_forward.node_idx_iter() {
        let edges = edges_fn(array_graph, node_idx);
        let from_name = array_graph.idx_to_name(node_idx);
        for edge in edges {
            let to_name = array_graph.idx_to_name(edge.points_to);
            let edge_type = match edge.flags.edge_type() {
                EdgeType::Tagged => "[T]",
                EdgeType::Dynamic => "[D]",
                EdgeType::Directed => "",
            };
            let line = format!("{from_name} -> {to_name} {edge_type}");
            result.push_str(line.trim());
            result.push('\n');
        }
    }
    result.trim().to_string()
}

pub fn print_arrows(array_graph: &ArrayGraph) -> String {
    let mut result = String::new();
    for node_idx in array_graph.edges_forward.node_idx_iter() {
        let arrows = array_graph.get_arrows_forward(node_idx).unwrap();
        for arrow in arrows {
            let from_name = array_graph.idx_to_name(arrow.points_from);
            let to_name = array_graph.idx_to_name(arrow.points_to);
            result.push_str(&format!("{from_name} -> {to_name}"));

            if let Some(tag) = &arrow.tag {
                result.push_str(&format!("\n   tag: {tag}"));
            }
            if let Some(branch) = &arrow.branch {
                result.push_str(&format!("\n   branch: {branch}"));
            }

            if let Some(properties) = &arrow.properties {
                result.push_str(&format!("\n   properties: {properties:?}"));
            }

            if let Some(message) = &arrow.message {
                result.push_str(&format!("\n   message: {message}"));
            }
            result.push('\n');
        }
    }
    result.trim().to_string()
}

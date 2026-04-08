// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod array_graph_test_trait;
pub mod traversal_config_test_trait;

use crate::ArrayGraph;
use crate::TwinArrow;
use crate::types::NodeIDX;
use crate::types::array_graph::Arrow;
use crate::types::array_graph::offset_graph::edge_flags::EdgeType;

pub fn idx_to_names<I: IntoIterator<Item = NodeIDX>>(graph: &ArrayGraph, idxs: I) -> Vec<String> {
    idxs.into_iter()
        .map(|idx| graph.idx_to_name(idx).to_string())
        .collect()
}

pub fn name_to_idx(graph: &ArrayGraph, name: &str) -> NodeIDX {
    graph.data.node_names_ordered.name_to_idx_log(name).unwrap()
}

pub fn print_all_node_names(graph: &ArrayGraph) -> String {
    graph
        .data
        .node_names_ordered
        .node_names_iter()
        .map(|name: &str| name.to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

pub fn print_forward_edges(array_graph: &ArrayGraph) -> String {
    let mut result = String::new();
    for node_idx in array_graph.node_idx_iter() {
        let from_name = array_graph.idx_to_name(node_idx);
        for (target, flags) in array_graph.forward_edges(node_idx) {
            let to_name = array_graph.idx_to_name(target);
            let edge_type = match flags.edge_type() {
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
    for node_idx in array_graph.node_idx_iter() {
        let arrows = array_graph.get_arrows_forward(node_idx).unwrap();
        for arrow in arrows {
            result.push_str(&print_arrow(array_graph, &arrow));
            result.push('\n');
        }
    }
    result.trim().to_string()
}

pub fn print_twin_arrows(ag: &ArrayGraph, twin_arrows: &Vec<TwinArrow>) -> String {
    let mut result = Vec::new();

    for twin_arrow in twin_arrows {
        let TwinArrow { l, r, .. } = twin_arrow;
        result.push(format!(
            "L: {}\n\nR: {}",
            l.as_ref().map(|a| print_arrow(ag, a)).unwrap_or_default(),
            r.as_ref().map(|a| print_arrow(ag, a)).unwrap_or_default()
        ));
    }
    result
        .join("\n\n--------\n\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect::<Vec<String>>()
        .join("\n")
}

pub fn print_arrow(array_graph: &ArrayGraph, arrow: &Arrow) -> String {
    let mut result = String::new();
    let from_name = array_graph.idx_to_name(arrow.points_from);
    let to_name = array_graph.idx_to_name(arrow.points_to);
    result.push_str(&format!("{from_name} -> {to_name}"));

    if let Some(tag) = &arrow.tag {
        result.push_str(&format!("\n   tag: {tag}"));
    }
    if let Some(dynamic) = &arrow.dynamic {
        result.push_str(&format!("\n   branch: {}", dynamic.branch));
        result.push_str(&format!(
            "\n   properties: {{\"type_key\": \"{}\", \"edge_name\": \"{}\"}}",
            dynamic.type_key, dynamic.edge_name
        ));
    }

    if let Some(message) = &arrow.message {
        result.push_str(&format!("\n   message: {message}"));
    }

    if arrow.skipped > 0 {
        result.push_str(&format!("\n   skipped: {}", arrow.skipped));
    }
    result.trim().to_string()
}

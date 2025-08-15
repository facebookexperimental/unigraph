use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::edge_flags::EdgeType;

pub struct ArrayGraphDebugUtils<'a>(pub &'a ArrayGraph);

impl<'a> ArrayGraphDebugUtils<'a> {
    pub fn to_forward_edges_string(&self) -> Result<String> {
        self.to_edges_string(|graph, node_idx| graph.edges_forward.edges(node_idx))
    }

    pub fn to_dom_edges_string(&self) -> Result<String> {
        self.to_edges_string(|graph, node_idx| graph.children_dominator(node_idx))
    }

    pub fn to_edges_string<FN>(&self, edges_fn: FN) -> Result<String>
    where
        FN: Fn(&ArrayGraph, NodeIDX) -> &[Edge],
    {
        let mut result = String::new();

        for node_idx in self.0.node_idx_iter() {
            let node_name = self.0.idx_to_name(node_idx);

            let mut tag_sets_str = String::new();
            if let Some(tag_sets) = self.0.tag_sets.get(&node_idx) {
                let mut tag_sets_strs = Vec::new();
                if !tag_sets.is_empty() {
                    for (tag_set, tags) in tag_sets {
                        let tags_str = tags.iter().cloned().collect::<Vec<_>>().join(", ");
                        tag_sets_strs.push(format!("{tag_set}: [{tags_str}]"));
                    }
                    tag_sets_str = tag_sets_strs.join(", ");
                    tag_sets_str = format!(" (tag sets: {tag_sets_str})");
                }
            }

            let unreachable_str = if self.0.node_flags[node_idx].contains(NodeFlags::UNREACHABLE) {
                " [UNREACHABLE]"
            } else {
                ""
            };

            result.push_str(&format!("{node_name}{unreachable_str}{tag_sets_str}:\n"));

            for edge in edges_fn(self.0, node_idx) {
                let points_to = self.0.idx_to_name(edge.points_to);
                let edge_type = match edge.flags.edge_type() {
                    EdgeType::Dynamic => " [D]",
                    EdgeType::Tagged => " [T]",
                    EdgeType::Directed => "",
                };

                result.push_str(&format!("  - {points_to}{edge_type}\n"));
            }
        }

        Ok(result.trim().to_string())
    }
}

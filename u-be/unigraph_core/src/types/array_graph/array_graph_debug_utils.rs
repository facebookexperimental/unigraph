use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::array_graph::offset_graph::edge_flags::EdgeType;

pub struct ArrayGraphDebugUtils<'a>(pub &'a ArrayGraph);

impl<'a> ArrayGraphDebugUtils<'a> {
    pub fn to_forward_edges_string(&self) -> Result<String> {
        self.to_edges_string(|graph, node_idx| {
            Box::new(graph.forward_edges(node_idx))
                as Box<dyn Iterator<Item = (NodeIDX, EdgeFlags)> + '_>
        })
    }

    pub fn to_dom_edges_string(&self) -> Result<String> {
        self.to_edges_string(|graph, node_idx| {
            Box::new(graph.children_dominator(node_idx))
                as Box<dyn Iterator<Item = (NodeIDX, EdgeFlags)> + '_>
        })
    }

    pub fn to_edges_string<FN>(&self, edges_fn: FN) -> Result<String>
    where
        FN: for<'b> Fn(
            &'b ArrayGraph,
            NodeIDX,
        ) -> Box<dyn Iterator<Item = (NodeIDX, EdgeFlags)> + 'b>,
    {
        let mut result = String::new();

        for node_idx in self.0.node_idx_iter() {
            let node_name = self.0.idx_to_name(node_idx);

            let mut labels_str = String::new();
            let node_labels = self.0.labels_for_node(node_idx);
            if !node_labels.is_empty() {
                let mut label_strs = Vec::new();
                for (label_name, values) in &node_labels {
                    let values_str = values.iter().cloned().collect::<Vec<_>>().join(", ");
                    label_strs.push(format!("{label_name}: [{values_str}]"));
                }
                labels_str = label_strs.join(", ");
                labels_str = format!(" (labels: {labels_str})");
            }

            let unreachable_str =
                if self.0.runtime.node_flags[node_idx].contains(NodeFlags::UNREACHABLE) {
                    " [UNREACHABLE]"
                } else {
                    ""
                };

            result.push_str(&format!("{node_name}{unreachable_str}{labels_str}:\n"));

            for (target, flags) in edges_fn(self.0, node_idx) {
                let points_to = self.0.idx_to_name(target);
                let edge_type = match flags.edge_type() {
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

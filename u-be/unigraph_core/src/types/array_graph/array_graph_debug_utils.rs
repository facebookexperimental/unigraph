use anyhow::Result;

use crate::ArrayGraph;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::EdgeFlags;

pub trait ArrayGraphDebugUtils {
    fn to_edges_string(&self) -> Result<String>;
}

impl ArrayGraphDebugUtils for ArrayGraph {
    fn to_edges_string(&self) -> Result<String> {
        let mut result = String::new();

        for node_idx in self.node_idx_iter() {
            let node_name = self.node_names_ordered.idx_to_name(node_idx);

            let mut tag_sets_str = String::new();
            if let Some(tag_sets) = self.tag_sets.get(&node_idx) {
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

            let unreachable_str = if self.node_flags[node_idx].contains(NodeFlags::UNREACHABLE) {
                " [UNREACHABLE]"
            } else {
                ""
            };

            result.push_str(&format!("{node_name}{unreachable_str}{tag_sets_str}:\n"));

            for edge in self.edges_forward.edges(node_idx) {
                let points_to = self.node_names_ordered.idx_to_name(edge.points_to);
                let edge_type = match edge
                    .flags
                    .intersection(EdgeFlags::IS_DYNAMIC | EdgeFlags::IS_TAGGED)
                {
                    EdgeFlags::IS_DYNAMIC => " [D]",
                    EdgeFlags::IS_TAGGED => " [T]",
                    _ => "",
                };

                result.push_str(&format!("  - {points_to}{edge_type}\n"));
            }
        }

        Ok(result.trim().to_string())
    }
}

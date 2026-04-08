use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializableEdges;
use crate::ArrayGraphSerializableNodeMetadata;
use crate::NodeIDX;
use crate::types::EdgeIDX;
use crate::types::EdgeMetaIDX;

/// Utility that takes a vec of sortable values, sorts the original vec in-place and returns
/// the context of original positions + mapping to the new positions.
pub fn sort_and_return_mapping<T: Ord>(vec: &mut Vec<T>) -> RemapContext {
    let new_vec = std::mem::take(vec);
    let mut vec_with_indices: Vec<(usize, T)> = new_vec.into_iter().enumerate().collect();
    vec_with_indices.sort_by(|a, b| a.1.cmp(&b.1));

    let mut sorted_vec = Vec::with_capacity(vec_with_indices.len());
    let mut mappings = vec![None; vec_with_indices.len()];
    let mut original_positions = Vec::with_capacity(vec_with_indices.len());

    for (new_position, (original_position, value)) in vec_with_indices.into_iter().enumerate() {
        sorted_vec.push(value);
        original_positions.push(Some(original_position.into()));
        mappings[original_position] = Some(new_position.into());
    }

    std::mem::swap(vec, &mut sorted_vec);

    RemapContext {
        original_positions,
        mappings,
    }
}

#[derive(Default)]
pub struct RemapContext {
    /// Original positions of the nodes in the new vec.
    /// if we have a vec like this:
    ///     vec!["C", "A", "B"];
    /// then we want to sort it and remap the indexes to make
    ///     vec!["A", "B", "C"];
    /// the original_positions vec will be:
    ///    vec![Some(1), Some(2), Some(0)];
    ///
    /// Some cases will not have the original postition. E.g. when we merge
    /// two graphs, the resulting graph will have some nodes that were not present
    /// in one of the original graphs.
    pub original_positions: Vec<Option<NodeIDX>>,
    /// Mappings represent the new positions of the original nodes.
    /// if we have a vec like this:
    ///     vec!["C", "A", "B"];
    /// then we want to sort it and remap the indexes to make
    ///     vec!["A", "B", "C"];
    /// the mappings vec will be:
    ///     vec![Some(2), Some(0), Some(1)];
    /// In the resulting vec, the position of the mapping element represents
    /// the original position of the thing in the original vec. The value represents
    /// the new position of the element in the new (sorted) vec (if any).
    ///
    /// For cases where we remap to a smaller graph (e.g. filtering a subgraph) we will
    /// have some nodes map to None, meaning that this node/idx should not appear in the
    /// new graph.
    pub mappings: Vec<Option<NodeIDX>>,
}

impl RemapContext {
    #[cfg(test)]
    pub fn debug(&self) -> String {
        let mut result = String::new();
        result.push_str(
            format!(
                "org: {}",
                self.original_positions
                    .iter()
                    .map(|opt| opt.map_or("_".to_string(), |idx| idx.0.to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .trim(),
        );
        result.push('\n');
        result.push_str(
            format!(
                "map: {}",
                self.mappings
                    .iter()
                    .map(|opt| opt.map_or("_".to_string(), |idx| idx.0.to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .trim(),
        );
        result.push('\n');
        result
    }

    pub fn get_original_position_assert(&self, node_idx: NodeIDX) -> Result<NodeIDX> {
        self.original_positions[node_idx].context(format!(
            "RemapContext: remapped Node index {node_idx} does not have an original position",
        ))
    }
}

pub fn remap_node_names_ordered(
    node_names_ordered: &ArrayGraphNodes,
    remap_context: &RemapContext,
) -> Result<ArrayGraphNodes> {
    let mut names = String::new();
    let mut offsets = vec![0];

    for &original_position in &remap_context.original_positions {
        let original_position = original_position.context(
            "When remapping node names ordered, we must have an original position of the string,
            since node names have to be ordered, unique and have no gaps",
        )?;
        let name = node_names_ordered.idx_to_name(original_position);
        names.push_str(name);
        offsets.push(names.len());
    }

    Ok(ArrayGraphNodes::from_parts(names, offsets))
}

pub fn remap_edges(
    edges: &ArrayGraphSerializableEdges,
    remap_context: &RemapContext,
) -> Result<ArrayGraphSerializableEdges> {
    let mut new_edges = Vec::with_capacity(edges.edges.len());
    let mut new_offsets = Vec::with_capacity(remap_context.original_positions.len() + 1);
    new_offsets.push(0);

    let new_edge_metadata = edges.edge_metadata.clone();
    let mut new_edge_metadata_map: BTreeMap<EdgeIDX, EdgeMetaIDX> = BTreeMap::new();

    for &original_position in &remap_context.original_positions {
        if let Some(original_position) = original_position {
            let range = edges.edge_range(original_position);
            for old_edge_idx in range {
                let old_target = edges.edges[old_edge_idx];
                if let Some(new_target) = remap_context.mappings[old_target] {
                    let new_edge_idx = EdgeIDX::from(new_edges.len());
                    new_edges.push(new_target);

                    // Remap metadata reference if this edge has one
                    if let Some(&meta_idx) =
                        edges.edge_metadata_map.get(&EdgeIDX::from(old_edge_idx))
                    {
                        new_edge_metadata_map.insert(new_edge_idx, meta_idx);
                    }
                }
            }
        }
        new_offsets.push(new_edges.len());
    }

    Ok(ArrayGraphSerializableEdges {
        edges: new_edges,
        edge_offsets: new_offsets,
        edge_metadata: new_edge_metadata,
        edge_metadata_map: new_edge_metadata_map,
    })
}

pub fn make_remapped_node_names_ordered(new_node_names: &[String]) -> ArrayGraphNodes {
    let mut names = String::new();
    let mut offsets = vec![0];

    for node_name in new_node_names {
        names.push_str(node_name);
        offsets.push(names.len());
    }

    ArrayGraphNodes::from_parts(names, offsets)
}

pub fn remap_node_metadata(
    metadata: &ArrayGraphSerializableNodeMetadata,
    ctx: &RemapContext,
) -> Result<ArrayGraphSerializableNodeMetadata> {
    let mut new_metrics = BTreeMap::new();

    for (metric_name, metrics) in &metadata.metrics {
        let mut new_vec = Vec::with_capacity(metrics.len());
        for &original_position in &ctx.original_positions {
            if let Some(original_position) = original_position {
                new_vec.push(metrics[original_position]);
            } else {
                // for nodes that don't exist we will use 0.0 as the default
                new_vec.push(0.0);
            }
        }
        new_metrics.insert(metric_name.clone(), new_vec);
    }

    // Remap labels: iterate by label name, remap node indices within each
    let mut new_labels = BTreeMap::new();
    for (label_name, node_map) in &metadata.labels {
        let mut new_node_map = BTreeMap::new();
        for (old_node_idx, values) in node_map {
            if let Some(new_node_idx) = ctx.mappings[*old_node_idx] {
                new_node_map.insert(new_node_idx, values.clone());
            }
        }
        if !new_node_map.is_empty() {
            new_labels.insert(label_name.clone(), new_node_map);
        }
    }

    // Remap properties: iterate by property name, remap node indices within each
    let mut new_properties = BTreeMap::new();
    for (prop_name, node_map) in &metadata.properties {
        let mut new_node_map = BTreeMap::new();
        for (old_node_idx, value) in node_map {
            if let Some(new_node_idx) = ctx.mappings[*old_node_idx] {
                new_node_map.insert(new_node_idx, value.clone());
            }
        }
        if !new_node_map.is_empty() {
            new_properties.insert(prop_name.clone(), new_node_map);
        }
    }

    Ok(ArrayGraphSerializableNodeMetadata {
        metrics: new_metrics,
        labels: new_labels,
        properties: new_properties,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;
    use crate::ArrayGraphSerializable;
    use crate::tests::test_graphs::make_test_array_graph_1;

    #[test]
    fn test_sort_and_return_mapping() {
        let mut input = vec![3, 1, 2];
        let ctx = sort_and_return_mapping(&mut input);
        assert_equal!(input, vec![1, 2, 3]);
        assert_equal!(
            ctx.mappings,
            vec![
                Some(NodeIDX::from(2u32)),
                Some(NodeIDX::from(0u32)),
                Some(NodeIDX::from(1u32))
            ]
        );
        assert_equal!(
            ctx.original_positions,
            vec![
                Some(NodeIDX::from(1u32)),
                Some(NodeIDX::from(2u32)),
                Some(NodeIDX::from(0u32))
            ]
        );
    }

    #[test]
    fn test_graph_remapping() -> Result<()> {
        let g = make_test_array_graph_1()?;
        snapshot!(
            g.debug().to_forward_edges_string()?,
            "
A:
  - B
  - D
B:
  - C [T]
  - J [T]
C (labels: disallow_tags: [b, c]):
D:
  - F
  - E [T]
E:
F:
  - G [D]
  - H [D]
  - I [D]
G:
H:
I:
J (labels: assert_tags: [a, b]):
"
        );

        // Make a new set of names prefixed with reverse indexes
        // to test the remapping functionality.
        let mut new_node_names = g
            .data
            .node_names_ordered
            .node_names_iter()
            .collect::<Vec<&str>>()
            .into_iter()
            .rev()
            .enumerate()
            .map(|(rev_idx, name)| format!("{rev_idx}_{name}"))
            .rev()
            .collect::<Vec<_>>();

        let sg = g.into_serializable();

        snapshot!(
            new_node_names.join(" "),
            "9_A 8_B 7_C 6_D 5_E 4_F 3_G 2_H 1_I 0_J"
        );

        let ctx = sort_and_return_mapping(&mut new_node_names);

        let new_sg = ArrayGraphSerializable {
            node_names_ordered: make_remapped_node_names_ordered(&new_node_names),
            edges: remap_edges(&sg.edges, &ctx)?,
            node_metadata: remap_node_metadata(&sg.node_metadata, &ctx)?,
            graph_settings: None,
            traversal_config: None,
            entry_points: None,
            properties: BTreeMap::new(),
        };

        let new_g = new_sg.into_array_graph(&ll::Task::create_new(""))?;
        snapshot!(
            new_g.debug().to_forward_edges_string()?,
            "
0_J (labels: assert_tags: [a, b]):
1_I:
2_H:
3_G:
4_F:
  - 3_G [D]
  - 2_H [D]
  - 1_I [D]
5_E:
6_D:
  - 4_F
  - 5_E [T]
7_C (labels: disallow_tags: [b, c]):
8_B:
  - 7_C [T]
  - 0_J [T]
9_A:
  - 8_B
  - 6_D
"
        );

        Ok(())
    }
}

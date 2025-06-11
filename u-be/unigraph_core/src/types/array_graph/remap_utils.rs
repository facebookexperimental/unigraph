use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraphDynamicEdge;
use crate::NodeIDX;
use crate::NodeNamesOrdered;
use crate::types::Tag;
use crate::types::array_graph::array_graph_serializable::ArrayGraphSerializableEdges;
use crate::types::array_graph::array_graph_serializable::ArrayGraphSerializableNodeMetadata;

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
        original_positions.push(original_position.into());
        mappings[original_position] = Some(new_position.into());
    }

    std::mem::swap(vec, &mut sorted_vec);

    RemapContext {
        original_positions,
        mappings,
    }
}

pub struct RemapContext {
    /// Original positions of the nodes in the new vec.
    /// if we have a vec like this:
    ///     vec!["C", "A", "B"];
    /// then we want to sort it and remap the indexes to make
    ///     vec!["A", "B", "C"];
    /// the original_positions vec will be:
    ///    vec![1, 2, 0];
    pub original_positions: Vec<NodeIDX>,
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

pub fn remap_node_names_ordered(
    node_names_ordered: &NodeNamesOrdered,
    remap_context: &RemapContext,
) -> NodeNamesOrdered {
    let mut names = String::new();
    let mut offsets = vec![0];

    for &original_position in &remap_context.original_positions {
        let name = node_names_ordered.idx_to_name(original_position);
        names.push_str(name);
        offsets.push(names.len());
    }

    NodeNamesOrdered::from_parts(names, offsets)
}

pub fn remap_edges(
    edges: &ArrayGraphSerializableEdges,
    remap_context: &RemapContext,
) -> Result<ArrayGraphSerializableEdges> {
    let (directed, directed_offsets) =
        remap_directed_edges(&edges.directed, &edges.directed_offsets, remap_context);

    Ok(ArrayGraphSerializableEdges {
        directed,
        directed_offsets,
        tagged: remap_tagged_edges(&edges.tagged, remap_context)
            .context("Failed to remap tagged edges")?,
        dynamic: remap_dynamic_edges(&edges.dynamic, remap_context)
            .context("Failed to remap dynamic edges")?,
    })
}

pub fn remap_directed_edges(
    edges: &[NodeIDX],
    offsets: &[usize],
    remap_context: &RemapContext,
) -> (Vec<NodeIDX>, Vec<usize>) {
    let mut remapped_edges = Vec::with_capacity(edges.len());
    let mut remapped_offsets = Vec::with_capacity(offsets.len());
    remapped_offsets.push(0);

    for &original_position in &remap_context.original_positions {
        let node_edges = &edges[offsets[original_position]..offsets[original_position + 1]];
        for &old_points_to in node_edges {
            if let Some(new_points_to) = remap_context.mappings[old_points_to] {
                remapped_edges.push(new_points_to);
            }
        }
        remapped_offsets.push(remapped_edges.len());
    }

    (remapped_edges, remapped_offsets)
}

fn remap_tagged_edges(
    tagged: &BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    remap_context: &RemapContext,
) -> Result<BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>> {
    let mut result: BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<NodeIDX>>> = BTreeMap::new();

    for (old_node_idx, edges) in tagged {
        if let Some(new_node_idx) = remap_context.mappings[*old_node_idx] {
            for (tag, points_to_set) in edges {
                for old_points_to in points_to_set {
                    if let Some(new_points_to) = remap_context.mappings[*old_points_to] {
                        result
                            .entry(new_node_idx)
                            .or_default()
                            .entry(tag.clone())
                            .or_default()
                            .insert(new_points_to);
                    }
                }
            }
        }
    }

    Ok(result)
}

pub fn make_remapped_node_names_ordered(new_node_names: &[String]) -> NodeNamesOrdered {
    let mut names = String::new();
    let mut offsets = vec![0];

    for node_name in new_node_names {
        names.push_str(node_name);
        offsets.push(names.len());
    }

    NodeNamesOrdered::from_parts(names, offsets)
}

fn remap_dynamic_edges(
    dynamic: &BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,
    remap_context: &RemapContext,
) -> Result<BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>> {
    let mut result: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>> = BTreeMap::new();

    for (old_node_idx, edges) in dynamic {
        if let Some(new_node_idx) = remap_context.mappings[*old_node_idx] {
            for edge in edges {
                let new_branches = edge
                    .branches
                    .iter()
                    .map(|(branch, node_idxs)| {
                        let new_node_idxs: BTreeSet<NodeIDX> = node_idxs
                            .iter()
                            .filter_map(|&node_idx| remap_context.mappings[node_idx])
                            .collect();
                        (branch.clone(), new_node_idxs)
                    })
                    .collect();
                result
                    .entry(new_node_idx)
                    .or_default()
                    .push(ArrayGraphDynamicEdge {
                        properties: edge.properties.clone(),
                        branches: new_branches,
                    });
            }
        }
    }
    Ok(result)
}

pub fn remap_node_metadata(
    metadata: &ArrayGraphSerializableNodeMetadata,
    ctx: &RemapContext,
) -> Result<ArrayGraphSerializableNodeMetadata> {
    let mut new_metrics = BTreeMap::new();
    let mut new_tag_sets = BTreeMap::new();

    for (metric_name, metrics) in &metadata.metrics {
        let mut new_vec = Vec::with_capacity(metrics.len());
        for &original_position in &ctx.original_positions {
            new_vec.push(metrics[original_position])
        }
        new_metrics.insert(metric_name.clone(), new_vec);
    }

    for (old_node_idx, tag_sets) in &metadata.tag_sets {
        if let Some(new_node_idx) = ctx.mappings[*old_node_idx] {
            // If the node was remapped, we need to insert it into the new tag sets.
            // We will use the new node index as the key.
            new_tag_sets.insert(new_node_idx, tag_sets.clone());
        }
    }

    Ok(ArrayGraphSerializableNodeMetadata {
        metrics: new_metrics,
        tag_sets: new_tag_sets,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;
    use crate::ArrayGraphSerializable;
    use crate::tests::make_test_array_graph_1;
    use crate::types::array_graph::array_graph_debug_utils::ArrayGraphDebugUtils;

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
                NodeIDX::from(1u32),
                NodeIDX::from(2u32),
                NodeIDX::from(0u32)
            ]
        );
    }

    #[test]
    fn test_graph_remapping() -> Result<()> {
        let g = make_test_array_graph_1()?;
        snapshot!(
            g.to_edges_string()?,
            "
A:
  - B
  - D
B:
  - C [T]
  - J [T]
C (tag sets: disallow_tags: [b, c]):
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
J (tag sets: assert_tags: [a, b]):
"
        );

        // Make a new set of names prefixed with reverse indexes
        // to test the remapping functionality.
        let mut new_node_names = g
            .node_names_ordered
            .iter_names()
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
            array_graph_settings: None,
            traversal_config: None,
        };

        let new_g = new_sg.into_array_graph();
        snapshot!(
            new_g.to_edges_string()?,
            "
0_J (tag sets: assert_tags: [a, b]):
1_I:
2_H:
3_G:
4_F:
  - 2_H [D]
  - 3_G [D]
  - 1_I [D]
5_E:
6_D:
  - 4_F
  - 5_E [T]
7_C (tag sets: disallow_tags: [b, c]):
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

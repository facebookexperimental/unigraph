// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;

use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::TwinGraph;
use crate::remap_utils::RemapContext;
use crate::types::array_graph::array_graph_nodes::ArrayGraphNodesForGraphSide;
use crate::types::twin_graph::NodeExistenceFlags;

pub fn merge_into_twin(
    left: ArrayGraphSerializable,
    right: ArrayGraphSerializable,
) -> Result<TwinGraph> {
    // let (node_names, ctx_l, ctx_r) =
    let (node_names, ctx_l, ctx_r) = left.node_names_ordered.merge(&right.node_names_ordered);

    let node_names = Arc::new(node_names);

    let remapped_left = remap_with_nodes(left, &ctx_l, Arc::clone(&node_names))?;
    let remapped_right = remap_with_nodes(right, &ctx_r, Arc::clone(&node_names))?;

    let mut existence = vec![NodeExistenceFlags::IN_BOTH; node_names.combined_nodes_len()];

    for node_idx in node_names.combined_node_idx_iter() {
        match (
            ctx_l.original_positions[node_idx],
            ctx_r.original_positions[node_idx],
        ) {
            (Some(_), Some(_)) => {} // no op. exists in both
            (Some(_), None) => existence[node_idx].mark_not_in_right(),
            (None, Some(_)) => existence[node_idx].mark_not_in_left(),
            (None, None) => {
                existence[node_idx].mark_not_in_left();
                existence[node_idx].mark_not_in_right();
            }
        }
    }

    let existence = Arc::new(existence);

    let shared_node_names_l = ArrayGraphNodesForGraphSide::new_with_existence(
        Arc::clone(&node_names),
        Arc::clone(&existence),
        crate::GraphSide::Left,
    );
    let shared_node_names_r = ArrayGraphNodesForGraphSide::new_with_existence(
        Arc::clone(&node_names),
        Arc::clone(&existence),
        crate::GraphSide::Right,
    );

    let mut left = remapped_left.into_array_graph();
    left.nodes = shared_node_names_l;
    let mut right = remapped_right.into_array_graph();
    right.nodes = shared_node_names_r;

    Ok(TwinGraph {
        l: left,
        r: Some(right),
        node_names,
    })
}

fn remap_with_nodes(
    graph: ArrayGraphSerializable,
    ctx: &RemapContext,
    shared_node_names: Arc<ArrayGraphNodes>,
) -> Result<ArrayGraphSerializable> {
    Ok(ArrayGraphSerializable {
        node_names_ordered: shared_node_names,
        edges: graph.edges.remap(ctx)?,
        node_metadata: graph.node_metadata.remap(ctx)?,
        graph_settings: graph.graph_settings,
        traversal_config: graph.traversal_config,
        entry_points: graph.entry_points,
    })
}

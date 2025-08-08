// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;

use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::TwinGraph;
use crate::remap_utils::RemapContext;

pub fn merge_into_twin(
    left: ArrayGraphSerializable,
    right: ArrayGraphSerializable,
) -> Result<TwinGraph> {
    // let (node_names, ctx_l, ctx_r) =
    let (node_names, ctx_l, ctx_r) = left.node_names_ordered.merge(&right.node_names_ordered);

    let node_names = Arc::new(node_names);

    let remapped_left = remap_with_nodes(left, &ctx_l, Arc::clone(&node_names))?;
    let remapped_right = remap_with_nodes(right, &ctx_r, Arc::clone(&node_names))?;

    let left = remapped_left.into_array_graph();
    let right = remapped_right.into_array_graph();
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

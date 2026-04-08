// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::EdgeMeta;
use crate::NodeIDX;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::Arrow;
use crate::types::array_graph::DynamicEdgeInfo;
use crate::types::array_graph::offset_graph::Edge;

fn render_message(
    ag: &ArrayGraph,
    edge: Edge,
    edge_metadata: Option<&EdgeMeta>,
    points_from: NodeIDX,
) -> Result<Option<String>> {
    if let Some(message_idx) = edge.flags.get_message_idx() {
        if let Some(msg) = ag.runtime.state.indexed_messages.get_by_idx(message_idx) {
            return Ok(Some(msg.render(ag, points_from, edge_metadata)?));
        } else {
            return Ok(Some(format!(
                "This edge contains a message about traversal with the index {message_idx},
but the template for that message was not found in the traversal config",
            )));
        }
    }
    Ok(None)
}

pub fn get_arrows(
    ag: &ArrayGraph,
    node_idx: NodeIDX,
    graph_structure: GraphStructure,
) -> Result<Vec<Arrow>> {
    let view = ag.edge_view(graph_structure);

    view.edges_with_metadata(node_idx)
        .map(|(edge, metadata)| edge_to_arrow(ag, node_idx, edge, metadata))
        .collect::<Result<Vec<Arrow>>>()
        .with_context(|| {
            format!(
                "Failed to get arrows for node {node_idx} in graph structure {graph_structure:?}"
            )
        })
}

pub fn edge_to_arrow(
    ag: &ArrayGraph,
    points_from: NodeIDX,
    edge: Edge,
    metadata: Option<&EdgeMeta>,
) -> Result<Arrow> {
    let excluded = edge.flags.is_excluded();
    if !edge.flags.is_tagged_or_dynamic() {
        Ok(Arrow {
            tag: None,
            dynamic: None,
            points_from,
            points_to: edge.points_to,
            excluded,
            message: render_message(ag, edge, metadata, points_from)?,
            skipped: 0,
        })
    } else {
        match metadata {
            None => {
                anyhow::bail!("Tagged/dynamic edge should have metadata")
            }
            Some(EdgeMeta::Tagged { tag }) => Ok(Arrow {
                tag: Some(tag.clone()),
                dynamic: None,
                points_from,
                points_to: edge.points_to,
                excluded,
                message: render_message(ag, edge, metadata, points_from)?,
                skipped: 0,
            }),
            Some(EdgeMeta::Dynamic {
                type_key,
                edge_name,
                branch,
                ..
            }) => Ok(Arrow {
                tag: None,
                dynamic: Some(DynamicEdgeInfo {
                    type_key: type_key.clone(),
                    edge_name: edge_name.clone(),
                    branch: branch.clone(),
                    metadata: None,
                }),
                points_from,
                points_to: edge.points_to,
                excluded,
                message: render_message(ag, edge, metadata, points_from)?,
                skipped: 0,
            }),
        }
    }
}

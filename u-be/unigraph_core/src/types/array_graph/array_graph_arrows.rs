// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::Arrow;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;

fn render_message(
    ag: &ArrayGraph,
    edge: Edge,
    edge_metadata: &NonDirectedEdgeMetadata,
    points_from: NodeIDX,
) -> Result<Option<String>> {
    if let Some(message_idx) = edge.flags.get_message_idx() {
        if let Some(msg) = ag.state.indexed_messages.get_by_idx(message_idx) {
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
    let offset_graph = match graph_structure {
        GraphStructure::Forward => &ag.edges_forward,
        GraphStructure::Reverse => &ag.derived_state.edges_reverse,
        GraphStructure::Dominator => ag.edges_dom(),
    };

    offset_graph
        .edges_with_metadata(node_idx)
        .map(|(edge, metadata)| {
            let excluded = edge.flags.is_excluded();
            if !edge.flags.is_tagged_or_dynamic() {
                Ok(Arrow {
                    tag: None,
                    branch: None,
                    properties: None,
                    points_from: node_idx,
                    points_to: edge.points_to,
                    excluded,
                    message: render_message(ag, edge, metadata, node_idx)?,
                })
            } else {
                match metadata {
                    NonDirectedEdgeMetadata::Directed => {
                        anyhow::bail!("Directed edge should not have metadata")
                    }
                    NonDirectedEdgeMetadata::Tagged { tag } => Ok(Arrow {
                        tag: Some(tag.clone()),
                        branch: None,
                        properties: None,
                        points_from: node_idx,
                        points_to: edge.points_to,
                        excluded,
                        message: render_message(ag, edge, metadata, node_idx)?,
                    }),
                    NonDirectedEdgeMetadata::Dynamic { properties, branch } => Ok(Arrow {
                        tag: None,
                        branch: Some(branch.clone()),
                        properties: Some(properties.clone()),
                        points_from: node_idx,
                        points_to: edge.points_to,
                        excluded,
                        message: render_message(ag, edge, metadata, node_idx)?,
                    }),
                }
            }
        })
        .collect::<Result<Vec<Arrow>>>()
        .with_context(|| {
            format!(
                "Failed to get arrows for node {node_idx} in graph structure {graph_structure:?}"
            )
        })
}

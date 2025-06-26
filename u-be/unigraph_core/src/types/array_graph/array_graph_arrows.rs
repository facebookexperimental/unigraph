// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::types::array_graph::Arrow;
use crate::types::array_graph::offset_graph::EdgeFlags;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;

pub fn get_arrows_forward(ag: &ArrayGraph, node_idx: NodeIDX) -> Result<Vec<Arrow>> {
    ag.edges_forward
        .edges_with_metadata(node_idx)
        .map(|(edge, metadata)| {
            let excluded = edge.flags.contains(EdgeFlags::EXCLUDED);
            if !edge
                .flags
                .intersects(EdgeFlags::IS_TAGGED | EdgeFlags::IS_DYNAMIC)
            {
                Ok(Arrow {
                    tag: None,
                    branch: None,
                    properties: None,
                    points_from: node_idx,
                    points_to: edge.points_to,
                    excluded,
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
                    }),
                    NonDirectedEdgeMetadata::Dynamic { properties, branch } => Ok(Arrow {
                        tag: None,
                        branch: Some(branch.clone()),
                        properties: Some(properties.clone()),
                        points_from: node_idx,
                        points_to: edge.points_to,
                        excluded,
                    }),
                }
            }
        })
        .collect()
}

pub fn get_arrows_dominator(ag: &ArrayGraph, node_idx: NodeIDX) -> Vec<Arrow> {
    ag.children_dominator(node_idx)
        .iter()
        .map(|edge| Arrow {
            tag: None,
            branch: None,
            properties: None,
            points_from: node_idx,
            points_to: edge.points_to,
            excluded: false,
        })
        .collect()
}

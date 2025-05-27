// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use super::MapGraph;
use super::map_graph::DynamicEdge;
use super::map_graph::GraphNode;

type NodeName = String;

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct GraphiteGraph {
    pub nodes: BTreeMap<NodeName, GraphiteNode>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct GraphiteNode {
    pub edges: Option<GraphiteEdges>,
    pub metrics: Option<BTreeMap<String, f64>>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct GraphiteEdges {
    pub directed: Option<BTreeSet<NodeName>>,
    pub tagged: Option<BTreeMap<String, BTreeSet<NodeName>>>,
    pub dynamic: Option<Vec<DynamicEdge>>,
}

impl GraphiteGraph {
    pub fn into_map_graph(self) -> Result<MapGraph> {
        let mut map_graph = MapGraph {
            nodes: BTreeMap::new(),
        };

        for (node_name, node) in self.nodes {
            let mut edges_directed = None;
            let mut edges_tagged = None;
            let mut edges_dynamic = None;

            if let Some(node_edges) = node.edges {
                edges_directed = node_edges.directed;
                edges_tagged = node_edges.tagged;
                edges_dynamic = node_edges.dynamic;
            }

            let size = node
                .metrics
                .as_ref()
                .and_then(|v| v.get("size"))
                .copied()
                .unwrap_or(0.0) as u32;

            map_graph.nodes.insert(
                node_name,
                GraphNode {
                    edges_directed,
                    edges_tagged,
                    edges_dynamic,
                    size: Some(size),
                },
            );
        }

        Ok(map_graph)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let graph: GraphiteGraph = serde_json::from_str(json).context("Failed to parse JSON")?;
        Ok(graph)
    }
}

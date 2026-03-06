// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

use crate::MapGraph;
use crate::types::map_graph::GraphNode;

pub struct GraphBuilder {
    graph: MapGraph,
}

impl GraphBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        GraphBuilder {
            graph: MapGraph {
                nodes: BTreeMap::new(),
                traversal_config: Default::default(),
                graph_settings: Default::default(),
                entry_points: Default::default(),
            },
        }
    }

    pub fn add_node(&mut self, name: String) {
        self.graph.nodes.insert(name.clone(), GraphNode::default());
    }

    pub fn add_node_if_not_exists(&mut self, name: String) {
        if !self.graph.nodes.contains_key(&name) {
            self.add_node(name);
        }
    }

    pub fn add_edge(&mut self, from: &str, to: &str) -> Result<()> {
        self.add_node_if_not_exists(to.to_string());
        self.add_node_if_not_exists(from.to_string());

        let node = self.graph.nodes.get_mut(from).context("Node not found")?;
        node.edges_directed
            .get_or_insert_default()
            .insert(to.to_string());
        Ok(())
    }

    pub fn build(self) -> MapGraph {
        self.graph
    }
}

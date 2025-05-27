// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(dead_code)]
use std::collections::BTreeMap;

use crate::ArrayGraph;
use crate::types::NodeIDX;
use crate::types::NodeName;
pub type Message = String;

#[derive(Clone)]
#[derive(serde::Serialize, serde::Deserialize)]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct Decision {
    pub follow: bool,
    pub message: Option<Message>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct TraversalConfig {
    pub force_nodes: BTreeMap<NodeName, Decision>,
    // From Node Name -> To Node Name -> Decision
    pub force_edges: BTreeMap<NodeName, BTreeMap<NodeName, Decision>>,
}

/// The version of TraversalConfig that is used for NodeIDX instead of string
/// names so we can use it in a context of an array graph.
pub struct TraversalConfigIDX {
    pub force_nodes: BTreeMap<NodeIDX, Decision>,
    pub force_edges: BTreeMap<NodeIDX, BTreeMap<NodeIDX, Decision>>,
}

impl TraversalConfig {
    pub fn index(&self, array_graph: &ArrayGraph) -> TraversalConfigIDX {
        let mut force_nodes = BTreeMap::new();
        for (name, decision) in &self.force_nodes {
            if let Some(idx) = array_graph.node_names.name_to_idx_log(name) {
                force_nodes.insert(idx, decision.clone());
            }
        }
        let mut force_edges = BTreeMap::new();
        for (from_node_name, decisions) in &self.force_edges {
            if let Some(from_idx) = array_graph.node_names.name_to_idx_log(from_node_name) {
                let inner_map = force_edges.entry(from_idx).or_insert(BTreeMap::new());
                for (to_node_name, decision) in decisions {
                    if let Some(to_idx) = array_graph.node_names.name_to_idx_log(to_node_name) {
                        inner_map.insert(to_idx, decision.clone());
                    }
                }
            }
        }
        TraversalConfigIDX {
            force_nodes,
            force_edges,
        }
    }
}

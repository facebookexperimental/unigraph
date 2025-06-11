// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::Result;
use maplit::btreemap;

use super::NodeIDX;
use super::Tag;
use super::TagSetName;
use super::array_graph::ArrayGraph;
use super::array_graph::ArrayGraphDynamicEdge;
use super::array_graph::Arrow;
use super::array_graph::NodeFlags;
use super::array_graph::node_names_ordered::NodeNamesOrderedBuilder;
use super::array_graph::offset_graph::OffsetGraphBuilder;
use crate::TraversalConfig;

type NodeName = String;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct MapGraph {
    pub nodes: BTreeMap<NodeName, GraphNode>,
    pub traversal_config: Option<TraversalConfig>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct GraphNode {
    #[serde(default = "MapGraphEdges::empty")]
    pub edges: MapGraphEdges,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// String -> String fields that hold extra data about the node.
    pub extra_fields: Option<BTreeMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// String -> Set<String> fields that can old multiple values for the same key.
    pub tag_sets: Option<BTreeMap<String, BTreeSet<String>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BTreeMap<String, f32>>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct MapGraphEdges {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directed: Option<BTreeSet<NodeName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagged: Option<BTreeMap<String, BTreeSet<NodeName>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<Vec<DynamicEdge>>,
}

impl MapGraphEdges {
    fn empty() -> Self {
        Self {
            directed: None,
            tagged: None,
            dynamic: None,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct DynamicEdge {
    pub properties: BTreeMap<String, String>,
    pub branches: BTreeMap<String, Vec<NodeName>>,
}

impl MapGraph {
    pub fn to_array_graph(&self) -> Result<ArrayGraph> {
        let mut sizes = Vec::new();

        let mut all_tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>> =
            BTreeMap::new();
        let mut all_node_names_set = self.nodes.keys().cloned().collect::<HashSet<_>>();
        for node in self.nodes.values() {
            all_node_names_set.extend(node.edge_names_iter().cloned());
        }

        let (node_names_ordered, name_to_idx_map) =
            NodeNamesOrderedBuilder::from_names(all_node_names_set);

        let mut offset_graph_builder = OffsetGraphBuilder::new();

        let mut all_tagged_edges: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>> =
            BTreeMap::new();
        let mut all_dynamic_edges: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>> = BTreeMap::new();

        for node_name in node_names_ordered.iter_names() {
            let node_idx = *name_to_idx_map
                .get(node_name)
                .with_context(|| format!("Node name not found: {node_name}"))?;

            // The node might not be there if there was an edge to a node that was not in the graph.
            if let Some(node) = self.nodes.get(node_name) {
                for (tag, edges) in node.edges.tagged.iter().flatten() {
                    all_tagged_edges.entry(node_idx).or_default().insert(
                        tag.clone(),
                        edges
                            .iter()
                            .filter_map(|edge| name_to_idx_map.get(edge))
                            .copied()
                            .collect::<BTreeSet<_>>(),
                    );
                }

                for dynamic in node.edges.dynamic.iter().flatten() {
                    let mut result = ArrayGraphDynamicEdge {
                        properties: dynamic.properties.clone(),
                        branches: BTreeMap::new(),
                    };

                    for (branch_name, edges) in dynamic.branches.iter() {
                        result.branches.insert(
                            branch_name.clone(),
                            edges
                                .iter()
                                .filter_map(|edge| name_to_idx_map.get(edge))
                                .copied()
                                .collect(),
                        );
                    }
                    all_dynamic_edges.entry(node_idx).or_default().push(result);
                }

                offset_graph_builder.push_node(node.arrow_iter().map(|a| Arrow {
                    tag: a.tag,
                    branch: a.branch,
                    properties: a.properties,
                    points_from: node_idx,
                    points_to: name_to_idx_map.get(&a.points_to).copied().unwrap(),
                    points_to_unreachable: false,
                    excluded: false,
                }));
                sizes.push(
                    node.metrics
                        .as_ref()
                        .map_or(0.0, |m| m.get("size").cloned().unwrap_or(0.0)),
                );
                if let Some(tag_sets) = &node.tag_sets {
                    all_tag_sets.insert(node_idx, tag_sets.clone());
                }
            } else {
                offset_graph_builder.push_node(vec![]);
                sizes.push(0.0);
            }
        }

        let forward_edges = offset_graph_builder.build();
        let reverse_edges = forward_edges.reverse();

        Ok(ArrayGraph {
            node_names_ordered,
            node_flags: vec![NodeFlags::empty(); forward_edges.node_count()],
            edges_forward: forward_edges,
            edges_reverse: reverse_edges,
            metrics: btreemap! {
                "size".to_string() => sizes,
            },
            edges_tagged: all_tagged_edges,
            edges_dynamic: all_dynamic_edges,
            tag_sets: all_tag_sets,
            traversal_config: self.traversal_config.clone(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let graph: MapGraph = serde_json::from_str(json).context("Failed to parse JSON")?;
        Ok(graph)
    }

    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string(self).context("Failed to serialize")?;
        Ok(json)
    }
}

static STATIC_EMPTY_TAGGED_EDGES: OnceLock<BTreeMap<String, BTreeSet<NodeName>>> = OnceLock::new();
static STATIC_EMPTY_DYNAMIC_EDGES: OnceLock<Vec<DynamicEdge>> = OnceLock::new();

impl GraphNode {
    pub fn directed_edges_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.edges
            .directed
            .as_ref()
            .map(|edges| edges.iter())
            .unwrap_or_default()
    }

    pub fn tagged_edge_names_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.edges
            .tagged
            .as_ref()
            .unwrap_or_else(|| STATIC_EMPTY_TAGGED_EDGES.get_or_init(BTreeMap::new))
            .values()
            .flat_map(|v| v.iter())
    }

    pub fn dynamic_edge_names_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.edges
            .dynamic
            .as_ref()
            .unwrap_or_else(|| STATIC_EMPTY_DYNAMIC_EDGES.get_or_init(Vec::new))
            .iter()
            .flat_map(|edge| edge.branches.values().flat_map(|v| v.iter()))
    }

    pub fn edge_names_iter(&self) -> impl Iterator<Item = &NodeName> {
        self.directed_edges_iter()
            .chain(self.tagged_edge_names_iter())
            .chain(self.dynamic_edge_names_iter())
    }

    pub fn arrow_iter<'a>(&'a self) -> impl Iterator<Item = NamedArrow> + 'a {
        let directed = self.directed_edges_iter().map(|points_to| NamedArrow {
            tag: None,
            branch: None,
            properties: None,
            points_to: points_to.clone(),
        });

        let tagged = self
            .edges
            .tagged
            .as_ref()
            .into_iter()
            .flatten()
            .map(|(tag, edges)| {
                edges.iter().map(move |points_to| NamedArrow {
                    tag: Some(tag.clone()),
                    branch: None,
                    properties: None,
                    points_to: points_to.clone(),
                })
            });

        let dynamic = self.edges.dynamic.as_ref().into_iter().flatten().map(|d| {
            d.branches.iter().flat_map(move |(branch, edges)| {
                edges.iter().map(move |points_to| NamedArrow {
                    tag: None,
                    branch: Some(branch.clone()),
                    properties: Some(d.properties.clone()),
                    points_to: points_to.clone(),
                })
            })
        });

        directed.chain(tagged.flatten()).chain(dynamic.flatten())
    }
}

/// Version of an Arrow that uses node name string instead of NodeIDX.
pub struct NamedArrow {
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub properties: Option<BTreeMap<String, String>>,
    pub points_to: NodeName,
}

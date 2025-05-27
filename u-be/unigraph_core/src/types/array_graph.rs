// Copyright (c) Meta Platforms, Inc. and affiliates.

pub(crate) mod node_names_ordered;
pub(crate) mod offset_graph;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use node_names_ordered::NodeNamesOrdered;
use offset_graph::Edge;
use offset_graph::EdgeFlags;
use offset_graph::NonDirectedEdgeMetadata;
use offset_graph::OffsetGraph;

use super::DynamicBranchName;
use super::MetricName;
use super::NodeIDX;
use super::Tag;
use super::TagSetName;
use crate::GraphBuilder;
use crate::traversal::TraversalConfig;

pub struct ArrayGraph {
    pub node_names: NodeNamesOrdered,

    pub edges_forward: OffsetGraph,
    pub edges_reverse: OffsetGraph,

    pub edges_tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    pub edges_dynamic: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,

    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub tag_sets: BTreeMap<TagSetName, BTreeMap<NodeIDX, BTreeSet<Tag>>>,
}

pub struct ArrayGraphDynamicEdge {
    pub branches: BTreeMap<DynamicBranchName, BTreeSet<NodeIDX>>,
    pub properties: BTreeMap<String, String>,
}

impl ArrayGraph {
    pub fn empty() -> Result<Self> {
        GraphBuilder::new().build().to_array_graph()
    }

    pub fn nodes_len(&self) -> usize {
        self.node_names.nodes_len()
    }

    pub fn is_empty(&self) -> bool {
        self.node_names.nodes_len() == 0
    }

    pub fn children(&self, node_idx: NodeIDX) -> &[Edge] {
        self.edges_forward.edges(node_idx)
    }

    pub fn node_idx_iter(&self) -> impl Iterator<Item = NodeIDX> {
        (0..self.node_names.nodes_len()).map(NodeIDX::from)
    }

    pub fn idx_to_name(&self, idx: NodeIDX) -> &str {
        self.node_names.idx_to_name(idx)
    }

    pub fn idxs_to_names(&self, idxs: &[NodeIDX]) -> Vec<&str> {
        idxs.iter()
            .map(|idx| self.node_names.idx_to_name(*idx))
            .collect()
    }

    // If we don't have entrypoings explicitly defined,
    // we can assume that any node with no parents is an entrypoint
    pub fn determine_entrypoints(&self) -> Vec<NodeIDX> {
        let mut entrypoints = Vec::new();
        for node_idx in self.node_idx_iter() {
            if self.edges_reverse.edges(node_idx).is_empty() {
                entrypoints.push(node_idx);
            }
        }
        entrypoints
    }

    pub fn get_transitive_metric_value(&self, node_idx: NodeIDX, metric_name: &str) -> Result<f32> {
        let mut total = 0.0;
        if let Some(metrics) = self.metrics.get(metric_name) {
            for node_idx in self.edges_forward.dfs(&[node_idx]) {
                let value = metrics[node_idx];
                total += value
            }
            Ok(total)
        } else {
            Ok(0.0)
        }
    }

    pub fn get_arrows_forward(&self, node_idx: NodeIDX) -> Result<Vec<Arrow>> {
        self.edges_forward
            .edges_with_metadata(node_idx)
            .map(|(edge, metadata)| {
                let excluded = edge.flags.contains(EdgeFlags::LEFT_EXCLUDED);
                if !edge
                    .flags
                    .intersects(EdgeFlags::LEFT_IS_TAGGED | EdgeFlags::LEFT_IS_DYNAMIC)
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

    pub fn apply_traversal_config(&mut self, traversal_config: &TraversalConfig) -> Result<()> {
        let indexed_config = traversal_config.index(self);

        for (parent_idx, edge, _metadata) in self.edges_forward.iter_edges_mut() {
            if let Some(force_to) = indexed_config.force_edges.get(&parent_idx) {
                if let Some(decision) = force_to.get(&edge.points_to) {
                    match decision.follow {
                        true => edge.flags.remove(EdgeFlags::LEFT_EXCLUDED),
                        false => edge.flags.insert(EdgeFlags::LEFT_EXCLUDED),
                    }
                }
            }
            if let Some(decision) = indexed_config.force_nodes.get(&edge.points_to) {
                match decision.follow {
                    true => edge.flags.remove(EdgeFlags::LEFT_EXCLUDED),
                    false => edge.flags.insert(EdgeFlags::LEFT_EXCLUDED),
                }
            }
        }

        Ok(())
    }
}

/// This is a more heavyweight struct describing an edge in the graph.
/// This can represent any edge (directed/tagged/dynamic).
/// This is meant to be used for more sparce operations, like rendering
/// edges in the UI, or for debugging. Since these are much heavier they
/// are not fit for heavy computations, like DFS/BFS, computing transitive
/// metrics, etc.
#[derive(ts_rs::TS)]
#[ts(export)]
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Arrow {
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub properties: Option<BTreeMap<String, String>>,
    pub points_from: NodeIDX,
    pub points_to: NodeIDX,
    pub excluded: bool,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use anyhow::Result;
    use k9::*;

    use crate::make_test_graph;
    use crate::types::NodeIDX;

    #[test]
    pub fn basic_test() -> Result<()> {
        let test_graph = make_test_graph()?;
        let array_graph = test_graph.to_array_graph()?;

        assert_equal!(array_graph.edges_forward.node_count(), 6);
        assert_equal!(array_graph.edges_forward.edges_len(), 13);

        assert_equal!(array_graph.edges_reverse.node_count(), 6);
        assert_equal!(array_graph.edges_reverse.edges_len(), 13);

        let children_names = |node_idx: u32| {
            array_graph
                .children(NodeIDX(node_idx))
                .iter()
                .map(|edge| array_graph.idx_to_name(edge.points_to))
                .collect::<Vec<_>>()
        };

        assert_equal!(array_graph.node_names.node_names, "ABCDEF");
        assert_equal!(children_names(0), vec!["B", "C"]);
        assert_equal!(children_names(5), vec!["D", "E"]);

        let find_name = |name: &str| {
            array_graph
                .node_names
                .name_to_idx_log(name)
                .map(|idx| idx.0)
        };

        assert_equal!(find_name("A"), Some(0));
        assert_equal!(find_name("B"), Some(1));
        assert_equal!(find_name("C"), Some(2));
        assert_equal!(find_name("D"), Some(3));
        assert_equal!(find_name("E"), Some(4));
        assert_equal!(find_name("F"), Some(5));
        assert_equal!(find_name("a"), None);
        assert_equal!(find_name("meow"), None);
        Ok(())
    }

    #[test]
    fn test_dfs() -> Result<()> {
        let test_graph = make_test_graph()?;
        let array_graph = test_graph.to_array_graph()?;

        let visited = array_graph
            .edges_forward
            .dfs(&[NodeIDX(0)])
            .collect::<BTreeSet<_>>();
        let expected = [0, 1, 2, 3, 4, 5].iter().map(NodeIDX::from).collect();
        assert_equal!(visited, expected);

        let visited = array_graph
            .edges_forward
            .dfs(&[NodeIDX(4)])
            .collect::<BTreeSet<_>>();
        let expected = [3, 4, 5].iter().map(NodeIDX::from).collect();
        assert_equal!(visited, expected);

        Ok(())
    }
}

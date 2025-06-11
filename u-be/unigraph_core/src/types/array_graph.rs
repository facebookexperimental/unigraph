// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod array_graph_debug_utils;
pub mod array_graph_serializable;
pub mod array_graph_settings;
mod array_graph_stats;
pub(crate) mod node_names_ordered;
pub(crate) mod offset_graph;
pub mod remap_utils;
mod to_map_graph;

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
use crate::ArrayGraphSerializable;
use crate::GraphBuilder;
use crate::MapGraph;
use crate::traversal::TraversalConfig;
use crate::traversal::apply_to_array_graph::apply_traversal_config_to_array_graph;
use crate::traversal::reachable_subgraph::get_reachable_subgraph_unconfigured;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::TierName;
use crate::types::array_graph::array_graph_stats::ArrayGraphStats;

pub struct ArrayGraph {
    pub node_names_ordered: NodeNamesOrdered,
    pub node_flags: Vec<NodeFlags>,

    pub edges_forward: OffsetGraph,
    pub edges_reverse: OffsetGraph,

    pub edges_tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    pub edges_dynamic: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,

    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>>,

    pub traversal_config: Option<TraversalConfig>,
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct NodeFlags: u32 {
        const UNREACHABLE = 0b0001_0000_0000;

        const TIER_0 =      0b0000_1000_0000;
        const TIER_1 =      0b0000_0100_0000;
        const TIER_2 =      0b0000_0010_0000;
        const TIER_3 =      0b0000_0001_0000;
        const TIER_4 =      0b0000_0000_1000;
        const TIER_5 =      0b0000_0000_0100;
        const TIER_6 =      0b0000_0000_0010;
        const TIER_7 =      0b0000_0000_0001;
        const ALL_TIERS =   0b0000_1111_1111;
    }
}

impl NodeFlags {
    pub fn tier_idx(self) -> usize {
        let tier_bits = self.intersection(NodeFlags::ALL_TIERS);
        match tier_bits {
            NodeFlags::TIER_0 => 0,
            NodeFlags::TIER_1 => 1,
            NodeFlags::TIER_2 => 2,
            NodeFlags::TIER_3 => 3,
            NodeFlags::TIER_4 => 4,
            NodeFlags::TIER_5 => 5,
            NodeFlags::TIER_6 => 6,
            NodeFlags::TIER_7 => 7,
            _ => panic!(
                "NodeFlags does not represent a tier {:?}",
                self.to_binary_string()
            ),
        }
    }

    pub fn to_binary_string(self) -> String {
        let binary = format!("{:016b}", self.bits());
        let mut result = String::with_capacity(19); // 16 digits + 3 separators
        for (i, c) in binary.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                result.push('_');
            }
            result.push(c);
        }
        result
    }

    #[inline(always)]
    pub fn is_node_unreachable(self) -> bool {
        self.intersects(NodeFlags::UNREACHABLE)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ArrayGraphDynamicEdge {
    pub branches: BTreeMap<DynamicBranchName, BTreeSet<NodeIDX>>,
    pub properties: BTreeMap<String, String>,
}

impl ArrayGraph {
    pub fn empty() -> Result<Self> {
        GraphBuilder::new().build().to_array_graph()
    }

    pub fn stats(&self) -> ArrayGraphStats {
        ArrayGraphStats::from_array_graph(self)
    }

    pub fn into_serializable(self) -> ArrayGraphSerializable {
        self.into()
    }

    pub fn to_map_graph(&self) -> Result<MapGraph> {
        to_map_graph::to_map_graph(self)
    }

    pub fn nodes_len(&self) -> usize {
        self.node_names_ordered.nodes_len()
    }

    pub fn is_empty(&self) -> bool {
        self.node_names_ordered.nodes_len() == 0
    }

    pub fn children(&self, node_idx: NodeIDX) -> &[Edge] {
        self.edges_forward.edges(node_idx)
    }

    pub fn node_idx_iter(&self) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.node_names_ordered.nodes_len()).map(NodeIDX::from)
    }

    pub fn idx_to_name(&self, idx: NodeIDX) -> &str {
        self.node_names_ordered.idx_to_name(idx)
    }

    #[inline(always)]
    pub fn is_node_unreachable(&self, node_idx: NodeIDX) -> bool {
        self.node_flags[node_idx].is_node_unreachable()
    }

    pub fn idxs_to_names(&self, idxs: &[NodeIDX]) -> Vec<&str> {
        idxs.iter()
            .map(|idx| self.node_names_ordered.idx_to_name(*idx))
            .collect()
    }

    pub fn get_reachable_subgraph_unconfigured(
        self,
        roots: &[NodeIDX],
    ) -> Result<ArrayGraphSerializable> {
        get_reachable_subgraph_unconfigured(self, roots)
    }

    pub fn apply_traversal_config(&mut self, traversal_config: TraversalConfig) -> Result<()> {
        apply_traversal_config_to_array_graph(self, traversal_config)?;
        Ok(())
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
        if self.is_node_unreachable(node_idx) {
            return Ok(0.0);
        }

        let mut total = 0.0;
        if let Some(metrics) = self.metrics.get(metric_name) {
            for node_idx in self.edges_forward.dfs_configured(&[node_idx]) {
                let value = metrics[node_idx];
                total += value
            }
            Ok(total)
        } else {
            Ok(0.0)
        }
    }

    pub fn get_tiers_names(&self) -> Vec<TierName> {
        match &self.traversal_config {
            Some(TraversalConfig {
                tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)),
                ..
            }) => ascending_tiers
                .tiers
                .iter()
                .map(|tier| tier.name.clone())
                .collect(),
            Some(TraversalConfig {
                tiered_traversal: None,
                ..
            }) => vec![],
            None => vec![],
        }
    }

    pub fn get_transitive_tiered_metric_values(
        &self,
        node_idx: NodeIDX,
        metric_name: &str,
    ) -> Result<BTreeMap<TierName, f32>> {
        let mut result = BTreeMap::new();

        if self.is_node_unreachable(node_idx) {
            return Ok(result);
        }

        let tier_config = self
            .traversal_config
            .as_ref()
            .and_then(|config| config.tiered_traversal.as_ref());

        let metrics = self.metrics.get(metric_name);

        match (metrics, tier_config) {
            (Some(metrics), Some(TieredTraversalConfig::AscendingTiers(ascending_tiers))) => {
                let mut tiered_metrics = vec![0.0; 8];

                for node_idx in self.edges_forward.dfs_configured(&[node_idx]) {
                    let value = metrics[node_idx];
                    let tier_idx = self.node_flags[node_idx].tier_idx();
                    tiered_metrics[tier_idx] += value;
                }
                for (tier_idx, value) in tiered_metrics.into_iter().enumerate() {
                    if value > 0.0 {
                        result.insert(
                            ascending_tiers.tier_idx_to_name(tier_idx)?.to_string(),
                            value,
                        );
                    }
                }

                Ok(result)
            }
            (None, _) => Ok(result),
            (_, None) => Ok(result),
        }
    }

    pub fn get_arrows_forward(&self, node_idx: NodeIDX) -> Result<Vec<Arrow>> {
        self.edges_forward
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
                        points_to_unreachable: self.is_node_unreachable(edge.points_to),
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
                            points_to_unreachable: self.is_node_unreachable(edge.points_to),
                            excluded,
                        }),
                        NonDirectedEdgeMetadata::Dynamic { properties, branch } => Ok(Arrow {
                            tag: None,
                            branch: Some(branch.clone()),
                            properties: Some(properties.clone()),
                            points_from: node_idx,
                            points_to: edge.points_to,
                            points_to_unreachable: self.is_node_unreachable(edge.points_to),
                            excluded,
                        }),
                    }
                }
            })
            .collect()
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
    /// Whether the node that this edge points to is unreachable from the
    /// graph entrypoints using configured traversal.
    pub points_to_unreachable: bool,
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

        assert_equal!(array_graph.node_names_ordered.node_names, "ABCDEF");
        assert_equal!(children_names(0), vec!["B", "C"]);
        assert_equal!(children_names(5), vec!["D", "E"]);

        let find_name = |name: &str| {
            array_graph
                .node_names_ordered
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
            .dfs_configured(&[NodeIDX(0)])
            .collect::<BTreeSet<_>>();
        let expected = [0u32, 1, 2, 3, 4, 5].iter().map(NodeIDX::from).collect();
        assert_equal!(visited, expected);

        let visited = array_graph
            .edges_forward
            .dfs_configured(&[NodeIDX(4)])
            .collect::<BTreeSet<_>>();
        let expected = [3u32, 4, 5].iter().map(NodeIDX::from).collect();
        assert_equal!(visited, expected);

        Ok(())
    }
}

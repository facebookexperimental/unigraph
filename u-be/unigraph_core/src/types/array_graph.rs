// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod array_graph_debug_utils;
mod array_graph_metrics;
pub mod array_graph_serializable;
mod array_graph_stats;
pub mod graph_settings;
pub(crate) mod node_names_ordered;
pub(crate) mod offset_graph;
pub mod remap_utils;
mod super_root;
mod tarjan_strongly_connected_components;
mod to_map_graph;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::OnceLock;

use anyhow::Context;
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
use crate::graph_settings::GraphSettings;
use crate::traversal::TraversalConfig;
use crate::traversal::apply_to_array_graph::apply_traversal_config_to_array_graph;
use crate::traversal::reachable_subgraph::get_reachable_subgraph_unconfigured;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::TierName;
use crate::types::array_graph::array_graph_metrics::CombinedMetricsForNodes;
use crate::types::array_graph::array_graph_metrics::get_metrics_sums_for_nodes;
use crate::types::array_graph::array_graph_metrics::get_metrics_sums_tiered_for_nodes;
use crate::types::array_graph::array_graph_metrics::get_transitive_metric_value;
use crate::types::array_graph::array_graph_metrics::get_transitive_tiered_metric_values;
use crate::types::array_graph::array_graph_metrics::parents_len_configured;
use crate::types::array_graph::array_graph_stats::ArrayGraphStats;
use crate::types::array_graph::offset_graph::lengauer_tarjan_dominator_tree::make_dominator_tree;

pub struct ArrayGraph {
    pub node_names_ordered: NodeNamesOrdered,
    pub node_flags: Vec<NodeFlags>,

    pub edges_forward: OffsetGraph,
    pub edges_reverse: OffsetGraph,
    /// Dominator tree is pretty expensive to compute and we normally only
    /// need it for when dominator tree views are enabled in the UI. We'll store
    /// it in a OnceLock so that it is computed lazyly and only when needed.
    pub edges_dom: OnceLock<OffsetGraph>,

    pub sccs: OnceLock<Vec<Vec<NodeIDX>>>,

    pub edges_tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    pub edges_dynamic: BTreeMap<NodeIDX, Vec<ArrayGraphDynamicEdge>>,

    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub tag_sets: BTreeMap<NodeIDX, BTreeMap<TagSetName, BTreeSet<Tag>>>,

    pub traversal_config: Option<TraversalConfig>,
    pub graph_settings: Option<GraphSettings>,
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

    pub fn append_super_root(self) -> Result<ArrayGraph> {
        super_root::append_super_root(self).context("Failed to append super root")
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

    pub fn edges_dom(&self) -> &OffsetGraph {
        self.edges_dom
            .get_or_init(|| make_dominator_tree(&self.edges_forward, &self.determine_entrypoints()))
    }

    pub fn children_dominator(&self, node_idx: NodeIDX) -> &[Edge] {
        self.edges_dom().edges(node_idx)
    }

    pub fn sccs(&self) -> &Vec<Vec<NodeIDX>> {
        self.sccs
            .get_or_init(|| tarjan_strongly_connected_components::SCCBuilder::new(self).build())
    }

    pub fn node_idx_iter(&self) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.node_names_ordered.nodes_len()).map(NodeIDX::from)
    }

    pub fn node_idx_iter_reachable(&self) -> impl Iterator<Item = NodeIDX> {
        self.node_idx_iter()
            .filter(|&node_idx| !self.is_node_unreachable(node_idx))
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
        get_transitive_metric_value(self, node_idx, metric_name)
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
        dominated: bool,
    ) -> Result<BTreeMap<TierName, f32>> {
        get_transitive_tiered_metric_values(self, node_idx, metric_name, dominated)
    }

    pub fn parents_len_configured(&self, node_idx: NodeIDX) -> usize {
        parents_len_configured(self, node_idx)
    }

    pub fn all_reachable_node_idxs(&self) -> Vec<NodeIDX> {
        self.node_idx_iter()
            .filter(|&node_idx| !self.is_node_unreachable(node_idx))
            .collect()
    }

    pub fn transitive_count_configured(&self, node_idx: NodeIDX) -> usize {
        if self.is_node_unreachable(node_idx) {
            0
        } else {
            self.edges_forward.dfs_configured(&[node_idx]).count()
        }
    }

    pub fn transitive_count_configured_dominated(&self, node_idx: NodeIDX) -> usize {
        if self.is_node_unreachable(node_idx) {
            0
        } else {
            self.edges_dom().dfs_configured(&[node_idx]).count()
        }
    }

    pub fn get_combined_metrics_for_nodes(
        &self,
        node_idxs: &[NodeIDX],
    ) -> Result<CombinedMetricsForNodes> {
        Ok(CombinedMetricsForNodes {
            metrics: get_metrics_sums_for_nodes(self, node_idxs)?,
            tiered_metrics: get_metrics_sums_tiered_for_nodes(self, node_idxs)?,
        })
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

    pub fn get_arrows_dominator(&self, node_idx: NodeIDX) -> Vec<Arrow> {
        self.children_dominator(node_idx)
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

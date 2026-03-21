// Copyright (c) Meta Platforms, Inc. and affiliates.

mod array_graph_arrows;
pub mod array_graph_debug_utils;
pub mod array_graph_derived_state;
mod array_graph_determine_entrypoints;
pub(crate) mod array_graph_metrics;
mod array_graph_name_search;
pub(crate) mod array_graph_nodes;
pub mod array_graph_state;
mod array_graph_stats;
pub mod budget_graph;
mod conjoint_cost;
pub mod graph_settings;
pub(crate) mod offset_graph;
pub mod remap_utils;
mod super_root;
mod tarjan_strongly_connected_components;
pub mod tiers;
mod to_map_graph;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use offset_graph::Edge;
use offset_graph::OffsetGraph;

use super::DynamicBranchName;
use super::DynamicEdgeName;
use super::DynamicTypeKey;
use super::LabelName;
use super::LabelValue;
use super::MetricName;
use super::NodeIDX;
use super::PropertyName;
use super::PropertyValue;
use super::Tag;
use crate::ArrayGraphDebugUtils;
use crate::ArrayGraphSerializable;
use crate::GraphBuilder;
use crate::MapGraph;
use crate::TraversalType;
use crate::graph_settings::GraphSettings;
use crate::graph_settings::GraphStructure;
use crate::traversal::TraversalConfig;
use crate::traversal::apply_to_array_graph::apply_traversal_config_to_array_graph;
use crate::traversal::reachable_subgraph::get_reachable_subgraph_unconfigured;
use crate::types::NodeName;
use crate::types::TierName;
pub use crate::types::array_graph::array_graph_arrows::edge_to_arrow;
use crate::types::array_graph::array_graph_arrows::get_arrows;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_determine_entrypoints::determine_entrypoints;
use crate::types::array_graph::array_graph_metrics::CombinedMetricsForNodes;
use crate::types::array_graph::array_graph_metrics::CountAllNodes;
use crate::types::array_graph::array_graph_metrics::get_combined_metrics_for_entry_points;
use crate::types::array_graph::array_graph_metrics::get_metrics_sums_for_nodes;
use crate::types::array_graph::array_graph_metrics::get_metrics_sums_tiered_for_nodes;
use crate::types::array_graph::array_graph_metrics::get_transitive_metric_value;
pub use crate::types::array_graph::array_graph_metrics::get_transitive_tiered_metric_values;
use crate::types::array_graph::array_graph_metrics::parents_len_configured;
use crate::types::array_graph::array_graph_nodes::ArrayGraphNodesForGraphSide;
use crate::types::array_graph::array_graph_nodes::NodeIDXsArcIter;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
use crate::types::array_graph::array_graph_stats::ArrayGraphStats;
use crate::types::array_graph::budget_graph::BudgetConfig;
use crate::types::array_graph::conjoint_cost::ConjointCost;
use crate::types::array_graph::offset_graph::lengauer_tarjan_dominator_tree::make_dominator_tree;
use crate::types::array_graph::offset_graph::shortest_path::shortest_path;
use crate::types::array_graph::tiers::ALL_TIER_FLAGS;
use crate::types::array_graph::tiers::TIER_FLAGS;

pub struct ArrayGraph {
    pub nodes: ArrayGraphNodesForGraphSide,
    pub node_flags: Vec<NodeFlags>,

    pub edges_forward: OffsetGraph,

    pub derived_state: ArrayGraphDerivedState,
    pub state: ArrayGraphState,

    pub edges_tagged: BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    pub edges_dynamic: BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,

    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub labels: BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    pub properties: BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,

    pub graph_settings: Option<GraphSettings>,

    pub budget_configs: BTreeMap<String, BudgetConfig>,

    /// If present, these graph will use these entrypoints instead
    /// of automatically determining them.
    pub entry_points: Option<BTreeSet<NodeName>>,
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct NodeFlags: u32 {
        const UNREACHABLE = 0b0000_0001;

        const TIER_IDX_0 =  TIER_FLAGS[0];
        const TIER_IDX_1 =  TIER_FLAGS[1];
        const TIER_IDX_2 =  TIER_FLAGS[2];
        const TIER_IDX_3 =  TIER_FLAGS[3];
        const ALL_TIERS =   ALL_TIER_FLAGS;
    }
}

impl NodeFlags {
    pub fn tier_idx(self) -> Option<usize> {
        let tier_bits = self.intersection(NodeFlags::ALL_TIERS);
        match tier_bits {
            NodeFlags::TIER_IDX_0 => Some(0),
            NodeFlags::TIER_IDX_1 => Some(1),
            NodeFlags::TIER_IDX_2 => Some(2),
            NodeFlags::TIER_IDX_3 => Some(3),
            _ => None,
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

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Debug,
    Clone,
    typegen::TypeGen,
    PartialEq,
    Eq,
    PartialOrd,
    Ord
)]
pub struct ArrayGraphDynamicEdge {
    pub branches: BTreeMap<DynamicBranchName, BTreeSet<NodeIDX>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
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

    pub fn append_super_root(self, force: bool) -> Result<ArrayGraph> {
        super_root::append_super_root(self, force).context("Failed to append super root")
    }

    pub fn to_map_graph(&self) -> Result<MapGraph> {
        to_map_graph::to_map_graph(self)
    }

    pub fn nodes_len(&self) -> usize {
        self.nodes.nodes_len
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.nodes_len == 0
    }

    pub fn children(&self, node_idx: NodeIDX) -> &[Edge] {
        self.edges_forward.edges(node_idx)
    }

    pub fn edges_dom(&self) -> &OffsetGraph {
        self.derived_state
            .edges_dom
            .get_or_init(|| make_dominator_tree(&self.edges_forward, &self.determine_entrypoints()))
    }

    pub fn children_dominator(&self, node_idx: NodeIDX) -> &[Edge] {
        self.edges_dom().edges(node_idx)
    }

    pub fn sccs(&self) -> &Vec<Vec<NodeIDX>> {
        self.derived_state
            .sccs
            .get_or_init(|| tarjan_strongly_connected_components::SCCBuilder::new(self).build())
    }

    pub fn conjoint_cost(&self) -> &ConjointCost {
        self.derived_state
            .conjoint_cost
            .get_or_init(|| ConjointCost::build(self).expect("Failed to build conjoint cost"))
    }

    #[inline(always)]
    pub fn node_idx_iter(&self) -> NodeIDXsArcIter {
        self.nodes.node_idx_iter()
    }

    pub fn node_idx_iter_reachable(&self) -> impl Iterator<Item = NodeIDX> {
        self.node_idx_iter()
            .filter(|&node_idx| !self.is_node_unreachable(node_idx))
    }

    #[inline(always)]
    pub fn idx_to_name<I>(&self, node_idx: I) -> &str
    where
        I: Into<NodeIDX> + Copy,
    {
        self.nodes.idx_to_name(node_idx.into())
    }

    /// Collect all labels for a specific node from the inverted labels index.
    pub fn labels_for_node(&self, node_idx: NodeIDX) -> BTreeMap<&str, &BTreeSet<LabelValue>> {
        self.labels
            .iter()
            .filter_map(|(label_name, node_map)| {
                node_map
                    .get(&node_idx)
                    .map(|values| (label_name.as_str(), values))
            })
            .collect()
    }

    #[inline(always)]
    pub fn is_node_unreachable(&self, node_idx: NodeIDX) -> bool {
        self.node_flags[node_idx].is_node_unreachable() || !self.node_exists(node_idx)
    }

    #[inline(always)]
    pub fn node_exists(&self, node_idx: NodeIDX) -> bool {
        self.nodes.node_exists(node_idx)
    }

    pub fn idxs_to_names(&self, idxs: &[NodeIDX]) -> Vec<&str> {
        idxs.iter()
            .map(|idx| self.nodes.idx_to_name(*idx))
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

    pub fn determine_entrypoints(&self) -> Vec<NodeIDX> {
        determine_entrypoints(self)
    }

    pub fn get_transitive_metric_value(
        &self,
        node_idx: NodeIDX,
        metric_name: &str,
        dominated: bool,
    ) -> Result<f32> {
        get_transitive_metric_value(self, node_idx, metric_name, dominated)
    }

    pub fn get_transitive_tiered_metric_values(
        &self,
        node_idx: NodeIDX,
        metric_name: &str,
        dominated: bool,
    ) -> Result<BTreeMap<TierName, f32>> {
        get_transitive_tiered_metric_values(self, node_idx, metric_name, dominated, CountAllNodes)
            .with_context(|| {
                format!(
                    "ag:get_transitive_tiered_metric_values for node_idx: `{}`, node_name: `{}`",
                    node_idx,
                    self.idx_to_name(node_idx)
                )
            })
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
        if self.is_node_unreachable(node_idx) || !self.node_exists(node_idx) {
            0
        } else {
            self.edges_forward.dfs_configured(&[node_idx]).count()
        }
    }

    pub fn transitive_count_configured_dominated(&self, node_idx: NodeIDX) -> usize {
        if self.is_node_unreachable(node_idx) || !self.node_exists(node_idx) {
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
            node_count: node_idxs.len(),
        })
    }

    pub fn get_combined_metrics_for_entry_points(
        &mut self,
        force_edge_include: Option<(NodeIDX, NodeIDX)>, // from -> to
    ) -> Result<CombinedMetricsForNodes> {
        get_combined_metrics_for_entry_points(self, force_edge_include)
            .context("Failed to get combined metrics for entry points")
    }

    pub fn node_tier_idx(&self, node_idx: NodeIDX) -> Option<usize> {
        self.node_flags[node_idx].tier_idx()
    }

    pub fn try_node_tier_idx(&self, node_idx: NodeIDX) -> Result<usize> {
        self.node_flags[node_idx].tier_idx().with_context(|| {
            format!(
                "Node does not have a tier assigned. Node name: `{}`, node_idx: `{}`",
                self.idx_to_name(node_idx),
                node_idx
            )
        })
    }

    pub fn get_arrows(
        &self,
        node_idx: NodeIDX,
        graph_structure: GraphStructure,
    ) -> Result<Vec<Arrow>> {
        get_arrows(self, node_idx, graph_structure)
    }

    pub fn get_arrows_forward(&self, node_idx: NodeIDX) -> Result<Vec<Arrow>> {
        get_arrows(self, node_idx, GraphStructure::Forward)
    }

    pub fn get_arrows_dominator(&self, node_idx: NodeIDX) -> Result<Vec<Arrow>> {
        get_arrows(self, node_idx, GraphStructure::Dominator)
    }

    pub fn get_arrows_reverse(&self, node_idx: NodeIDX) -> Result<Vec<Arrow>> {
        get_arrows(self, node_idx, GraphStructure::Reverse)
    }

    pub fn search_name_fuzzy<'a>(
        &'a self,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<(&'a str, NodeIDX)>> {
        self.nodes.node_names.search_name_fuzzy(pattern, limit)
    }

    pub fn debug(&self) -> ArrayGraphDebugUtils<'_> {
        ArrayGraphDebugUtils(self)
    }

    pub fn shortest_path(
        &self,
        from: &[NodeIDX],
        to: NodeIDX,
        graph_structure: GraphStructure,
        traversal_type: TraversalType,
    ) -> Option<Vec<NodeIDX>> {
        let offset_graph = match graph_structure {
            GraphStructure::Forward => &self.edges_forward,
            GraphStructure::Reverse => &self.derived_state.edges_reverse,
            GraphStructure::Dominator => self.edges_dom(),
        };
        shortest_path(offset_graph, from, to, traversal_type)
    }
}

/// Dynamic-edge-only fields. None for directed/tagged edges.
/// Shared between Arrow (ArrayGraph level) and NamedArrow (MapGraph level).
#[derive(serde::Deserialize, serde::Serialize, typegen::TypeGen, Clone, Debug)]
pub struct DynamicEdgeInfo {
    pub type_key: DynamicTypeKey,
    pub edge_name: DynamicEdgeName,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

/// This is a more heavyweight struct describing an edge in the graph.
/// This can represent any edge (directed/tagged/dynamic).
/// This is meant to be used for more sparce operations, like rendering
/// edges in the UI, or for debugging. Since these are much heavier they
/// are not fit for heavy computations, like DFS/BFS, computing transitive
/// metrics, etc.
#[derive(serde::Deserialize, serde::Serialize, typegen::TypeGen)]
pub struct Arrow {
    pub tag: Option<String>,
    pub dynamic: Option<DynamicEdgeInfo>,
    pub points_from: NodeIDX,
    pub points_to: NodeIDX,
    pub excluded: bool,
    pub message: Option<String>,
    /// Relevant only for cases where arrows represent compressed path.
    /// e.g. when we show Changed Nodes only each row in the tree table
    /// represent a path from one node to another with potential nodes
    /// in between skipped. This value will represent how many nodes
    /// were skipped (shortest path)
    /// 0 means direct edge.
    ///
    /// Example:
    ///
    /// Actual Graph:
    ///         A
    ///       /  \
    ///      B    C
    ///       \  /
    ///         D     <- only changed node
    ///
    /// Graph with changed nodes only:
    ///         A
    ///         |
    ///         D     <- only changed node
    ///
    /// The arrow will look like: { from: A, to: D, skipped: 1 }
    /// where `1` means that D is at least 1 skippe node away from A
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use anyhow::Result;
    use k9::*;

    use crate::make_test_graph;
    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::name_to_idx;
    use crate::types::NodeIDX;

    #[test]
    pub fn basic_test() -> Result<()> {
        let test_graph = make_test_graph()?;
        let array_graph = test_graph.to_array_graph()?;

        assert_equal!(array_graph.edges_forward.node_count(), 6);
        assert_equal!(array_graph.edges_forward.edges_len(), 13);

        assert_equal!(array_graph.derived_state.edges_reverse.node_count(), 6);
        assert_equal!(array_graph.derived_state.edges_reverse.edges_len(), 13);

        let children_names = |node_idx: u32| {
            array_graph
                .children(NodeIDX(node_idx))
                .iter()
                .map(|edge| array_graph.idx_to_name(edge.points_to))
                .collect::<Vec<_>>()
        };

        assert_equal!(array_graph.nodes.node_names.node_names, "ABCDEF");
        assert_equal!(children_names(0), vec!["B", "C"]);
        assert_equal!(children_names(5), vec!["D", "E"]);

        let find_name = |name: &str| array_graph.nodes.name_to_idx_log(name).map(|idx| idx.0);

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

    #[test]
    fn transitive_count_test() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let a_idx = name_to_idx(&ag, "A");
        assert_equal!(ag.transitive_count_configured(a_idx), 11);

        let b_idx = name_to_idx(&ag, "B");
        assert_equal!(ag.transitive_count_configured(b_idx), 4);
        let c_idx = name_to_idx(&ag, "C");
        assert_equal!(ag.transitive_count_configured(c_idx), 1);
        Ok(())
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

mod changed_nodes_graph;
mod diff;
pub(crate) mod get_arrows;
mod merge;
mod metrics;
use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
pub use diff::NodeDiff;

use crate::ArrayGraph;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::graph_settings::GraphStructure;
use crate::types::TierName;
use crate::types::array_graph::array_graph_nodes::GraphSide;
use crate::types::twin_graph::changed_nodes_graph::ChangedNodesGraph;
use crate::types::twin_graph::get_arrows::TwinArrow;
use crate::types::twin_graph::metrics::get_transitive_tiered_delta;

const MISSING_RIGHT_ERROR: &str = "TwinGraph: You are trying to access the right graph, but it is not present. \
     Please ensure that the TwinGraph was initialized with a right graph.";

/// TwinGraph is a struct that represent a pair of graph that we normally
/// compare to each other.
/// The most common use case is to compare dependency graphs of the same codebase
/// at two different points in time, e.g. before and after a refactor/pull request/commit.
/// We use this struct as a first class citizen even if we only have one graph.
#[readonly::make]
pub struct TwinGraph {
    #[readonly]
    pub node_names: Arc<ArrayGraphNodes>,
    pub node_diff: Arc<Vec<NodeDiff>>,

    /// Left graph must always be present.
    #[readonly]
    pub l: ArrayGraph,
    pub r: Option<ArrayGraph>,
    changed_nodes: Option<ChangedNodesGraph>,
    #[readonly]
    pub metric_names: Vec<String>,
}

impl TwinGraph {
    pub fn from_one(l: ArrayGraph) -> Result<Self> {
        Ok(Self {
            node_names: Arc::clone(&l.nodes.node_names),
            node_diff: Arc::clone(&l.nodes.node_diff),
            changed_nodes: Some(ChangedNodesGraph::new()),
            metric_names: l.metrics.keys().cloned().collect(),
            l,
            r: None,
        })
    }

    pub fn from_two(l: ArrayGraphSerializable, r: ArrayGraphSerializable) -> Result<Self> {
        merge::merge_into_twin(l, r)
    }

    pub fn graph(&self, side: GraphSide) -> Result<&ArrayGraph> {
        match side {
            GraphSide::Left => Ok(&self.l),
            GraphSide::Right => self.r.as_ref().context(MISSING_RIGHT_ERROR),
        }
    }

    pub fn graph_mut(&mut self, side: GraphSide) -> Result<&mut ArrayGraph> {
        match side {
            GraphSide::Left => Ok(&mut self.l),
            GraphSide::Right => self.r.as_mut().context(MISSING_RIGHT_ERROR),
        }
    }

    pub fn graph_u32(&self, side: u32) -> Result<&ArrayGraph> {
        GraphSide::from_u32(side)
            .and_then(|s| self.graph(s))
            .context("graph_u32: Invalid GraphSide value")
    }

    pub fn graph_u32_mut(&mut self, side: u32) -> Result<&mut ArrayGraph> {
        GraphSide::from_u32(side)
            .and_then(|s| self.graph_mut(s))
            .context("graph_u32: Invalid GraphSide value")
    }

    pub fn get_twin_arrows(
        &self,
        node_idx: NodeIDX,
        graph_structure: GraphStructure,
        changed_node_only: bool,
    ) -> Result<Vec<TwinArrow>> {
        if changed_node_only {
            Ok(self
                .changed_nodes
                .as_ref()
                // if there is no changed nodes graph we return an empty result.
                // (likely because we only have one graph and the client wrongly asked
                // for changed nodes only but we don't want the UI to blow up)
                .map(|c| c.get_twin_arrows(self, node_idx, graph_structure))
                .transpose()?
                .unwrap_or_default())
        } else {
            get_arrows::get_twin_arrows(self, node_idx, graph_structure)
        }
        .context("get_arrow_pairs")
    }

    pub fn search_name_fuzzy<'a>(
        &'a self,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<(&'a str, NodeIDX)>> {
        self.node_names.search_name_fuzzy(pattern, limit)
    }

    pub fn get_transitive_count_delta(&self, node_idx: NodeIDX) -> Result<i32> {
        if let Some(r) = &self.r {
            metrics::get_transitive_count_delta(&self.l, r, node_idx)
        } else {
            Ok(0)
        }
    }

    pub fn get_transitive_tiered_delta(
        &self,
        node_idx: NodeIDX,
        metric_name: &str,
    ) -> Result<BTreeMap<TierName, f32>> {
        get_transitive_tiered_delta(self, node_idx, metric_name)
    }

    pub fn shortest_path(
        &self,
        from: &[NodeIDX],
        to: NodeIDX,
        graph_structure: GraphStructure,
        traversal_type: crate::types::array_graph::offset_graph::TraversalType,
        changed_node_only: bool,
    ) -> Result<Option<Vec<NodeIDX>>> {
        {
            if let (Some(changed_nodes), true) = (&self.changed_nodes, changed_node_only) {
                changed_nodes.shortest_path(self, from, to, graph_structure, traversal_type)
            } else {
                if let Some(r) = &self.r {
                    let left_path = self
                        .l
                        .shortest_path(from, to, graph_structure, traversal_type);
                    let right_path = r.shortest_path(from, to, graph_structure, traversal_type);

                    match (left_path, right_path) {
                        (Some(l), Some(r)) => {
                            // if both sides have a path, return the shorter one
                            if l.len() <= r.len() {
                                Ok(Some(l))
                            } else {
                                Ok(Some(r))
                            }
                        }
                        (None, Some(right_path)) => Ok(Some(right_path)),
                        (Some(left_path), None) => Ok(Some(left_path)),
                        (None, None) => Ok(None),
                    }
                } else {
                    Ok(self
                        .l
                        .shortest_path(from, to, graph_structure, traversal_type))
                }
            }
        }
        .context("TwinGraph::shortest_path")
    }
}

#[cfg(test)]
mod tests {
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_twin_graph;
    use crate::tests::test_utils::idx_to_names;

    #[test]
    fn test_twin_graphs() -> Result<()> {
        let tg = make_twin_graph()?;

        let l = &tg.l;
        let r = tg.graph(GraphSide::Right)?;

        let entrypoints_l = l.determine_entrypoints();
        let entrypoints_r = r.determine_entrypoints();

        snapshot!(
            idx_to_names(l, entrypoints_l),
            r#"
[
    "A",
    "L",
]
"#
        );
        snapshot!(
            idx_to_names(r, entrypoints_r),
            r#"
[
    "A",
    "L",
]
"#
        );

        assert_equal!(l.all_reachable_node_idxs().len(), 16);
        assert_equal!(r.all_reachable_node_idxs().len(), 20);
        Ok(())
    }
}

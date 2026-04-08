// Copyright (c) Meta Platforms, Inc. and affiliates.

mod changed_nodes_graph;
mod diff;
pub(crate) mod get_arrows;
mod merge;
mod metrics;
pub mod twin_remap;
use std::collections::BTreeMap;

use anyhow::Result;
pub use diff::NodeDiff;
pub use twin_remap::TwinRemap;

use crate::ArrayGraph;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::graph_settings::GraphStructure;
use crate::types::TierName;
use crate::types::twin_graph::changed_nodes_graph::ChangedNodesGraph;
use crate::types::twin_graph::get_arrows::TwinArrow;
use crate::types::twin_graph::metrics::get_transitive_tiered_delta;

/// Enum that represents one of the sides of the twin graph, either left graph or right graph.
#[derive(Clone, Copy, Debug)]
pub enum GraphSide {
    Left = 0b0001,
    Right = 0b0010,
}

impl GraphSide {
    pub fn from_u32(value: u32) -> Result<Self> {
        match value {
            0b0001 => Ok(GraphSide::Left),
            0b0010 => Ok(GraphSide::Right),
            _ => anyhow::bail!("Invalid GraphSide value: {}", value),
        }
    }
}

/// TwinGraph represents a pair of graphs being compared.
/// Both graphs keep their own independent node namespace.
/// TwinGraph owns a `TwinRemap` that translates between the merged (UI-facing)
/// index space and each side's local index space.
#[readonly::make]
pub struct TwinGraph {
    pub remap: TwinRemap,
    pub node_diff: Vec<NodeDiff>,
    pub l: ArrayGraph,
    #[readonly]
    pub r: ArrayGraph,
    changed_nodes: ChangedNodesGraph,
    #[readonly]
    pub metric_names: Vec<String>,
}

impl TwinGraph {
    pub fn from_two(
        l: ArrayGraphSerializable,
        r: ArrayGraphSerializable,
        task: &ll::Task,
    ) -> Result<Self> {
        merge::merge_into_twin(l, r, task)
    }

    pub fn merged_len(&self) -> usize {
        self.remap.merged_len
    }

    pub fn merged_node_idx_iter(
        &self,
    ) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.merged_len()).map(NodeIDX::from)
    }

    /// Translate a merged index to a local index on the given side.
    /// Returns None if the node doesn't exist on that side.
    pub fn to_local(&self, side: GraphSide, merged_idx: NodeIDX) -> Option<NodeIDX> {
        match side {
            GraphSide::Left => self.remap.twin_to_l[merged_idx],
            GraphSide::Right => self.remap.twin_to_r[merged_idx],
        }
    }

    /// Translate a local index to a merged index.
    pub fn to_merged(&self, side: GraphSide, local_idx: NodeIDX) -> NodeIDX {
        match side {
            GraphSide::Left => self.remap.l_to_twin[usize::from(local_idx)],
            GraphSide::Right => self.remap.r_to_twin[usize::from(local_idx)],
        }
    }

    /// Get the name for a merged index.
    pub fn merged_idx_to_name(&self, merged_idx: NodeIDX) -> &str {
        self.remap.merged_idx_to_name(
            &self.l.data.node_names_ordered,
            &self.r.data.node_names_ordered,
            merged_idx,
        )
    }

    pub fn graph(&self, side: GraphSide) -> &ArrayGraph {
        match side {
            GraphSide::Left => &self.l,
            GraphSide::Right => &self.r,
        }
    }

    pub fn graph_mut(&mut self, side: GraphSide) -> &mut ArrayGraph {
        match side {
            GraphSide::Left => &mut self.l,
            GraphSide::Right => &mut self.r,
        }
    }

    pub fn graph_u32(&self, side: u32) -> Result<&ArrayGraph> {
        Ok(self.graph(GraphSide::from_u32(side)?))
    }

    pub fn graph_u32_mut(&mut self, side: u32) -> Result<&mut ArrayGraph> {
        Ok(self.graph_mut(GraphSide::from_u32(side)?))
    }

    pub fn to_local_u32(&self, side: u32, merged_idx: NodeIDX) -> Result<Option<NodeIDX>> {
        let side = GraphSide::from_u32(side)?;
        Ok(self.to_local(side, merged_idx))
    }

    pub fn get_twin_arrows(
        &self,
        node_idx: NodeIDX,
        graph_structure: GraphStructure,
        changed_node_only: bool,
    ) -> Result<Vec<TwinArrow>> {
        if changed_node_only {
            self.changed_nodes
                .get_twin_arrows(self, node_idx, graph_structure)
        } else {
            get_arrows::get_twin_arrows(self, node_idx, graph_structure)
        }
    }

    pub fn search_name_fuzzy<'a>(
        &'a self,
        pattern: &str,
        limit: usize,
        task: &ll::Task,
    ) -> Result<Vec<(&'a str, NodeIDX)>> {
        let results = self
            .r
            .data
            .node_names_ordered
            .search_name_fuzzy(pattern, limit, task)?;
        Ok(results
            .into_iter()
            .map(|(name, local_idx)| (name, self.remap.r_to_twin[usize::from(local_idx)]))
            .collect())
    }

    pub fn get_transitive_count_delta(&self, node_idx: NodeIDX) -> Result<i32> {
        metrics::get_transitive_count_delta(self, &self.l, &self.r, node_idx)
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
        if changed_node_only {
            return self.changed_nodes.shortest_path(
                self,
                from,
                to,
                graph_structure,
                traversal_type,
            );
        }

        let l_from = self.translate_idxs(GraphSide::Left, from);
        let r_from = self.translate_idxs(GraphSide::Right, from);

        let l_to = self.to_local(GraphSide::Left, to);
        let r_to = self.to_local(GraphSide::Right, to);

        let left_path = l_to.and_then(|to_local| {
            self.l
                .shortest_path(&l_from, to_local, graph_structure, traversal_type)
        });
        let right_path = r_to.and_then(|to_local| {
            self.r
                .shortest_path(&r_from, to_local, graph_structure, traversal_type)
        });

        let left_path = left_path.map(|p| self.translate_path(GraphSide::Left, &p));
        let right_path = right_path.map(|p| self.translate_path(GraphSide::Right, &p));

        match (left_path, right_path) {
            (Some(l), Some(r)) => {
                if l.len() <= r.len() {
                    Ok(Some(l))
                } else {
                    Ok(Some(r))
                }
            }
            (None, Some(r)) => Ok(Some(r)),
            (Some(l), None) => Ok(Some(l)),
            (None, None) => Ok(None),
        }
    }

    fn translate_idxs(&self, side: GraphSide, merged_idxs: &[NodeIDX]) -> Vec<NodeIDX> {
        merged_idxs
            .iter()
            .filter_map(|&idx| self.to_local(side, idx))
            .collect()
    }

    fn translate_path(&self, side: GraphSide, local_path: &[NodeIDX]) -> Vec<NodeIDX> {
        local_path
            .iter()
            .map(|&idx| self.to_merged(side, idx))
            .collect()
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

        let l = tg.graph(GraphSide::Left);
        let r = &tg.r;

        let entrypoints_l = l.determine_entrypoints();
        let entrypoints_r = r.determine_entrypoints();

        snapshot!(
            idx_to_names(l, entrypoints_l),
            r#"
[
    "\u{10ffff}__root__\u{10ffff}",
]
"#
        );
        snapshot!(
            idx_to_names(r, entrypoints_r),
            r#"
[
    "\u{10ffff}__root__\u{10ffff}",
]
"#
        );

        assert_equal!(l.all_reachable_node_idxs().len(), 17);
        assert_equal!(r.all_reachable_node_idxs().len(), 21);
        Ok(())
    }
}

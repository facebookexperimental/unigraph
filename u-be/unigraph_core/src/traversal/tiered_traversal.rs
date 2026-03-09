use std::collections::BTreeMap;
use std::collections::HashSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::types::NodeIDX;
use crate::types::Tag;
use crate::types::TierIDX;
use crate::types::TierName;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::OffsetGraph;
use crate::types::array_graph::tiers::tier_idx_to_flags;

/// Configuration for tiered traversal, which allows traversing the graph in tiers.
/// Specific use case for this is JavaScript loading tiers. E.g. initial payload vs.
/// lazyloaded JS.
/// When we traverse the graph we look at the tagged edges. If the edge has a tag
/// we look at the node's current tier and then we look at the new tier this node
/// is supposed to transition to and record that.
#[derive(Debug, typegen::TypeGen, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize, Clone, unigraph_delta::Deltable)]
#[deltable(replace)]
pub enum TieredTraversalConfig {
    // Simple ascending tiers configuration.
    // This is specifically used for JS loading tiers.
    // Certain tagged edges will transition from one tier to another.
    // We can only transition up, not down. e.g., once you LazyLoad (second tier)
    // a JS module, everything past that tier will be considered lazyloaded
    // You can't go back to the initial tier.
    AscendingTiers(AscendingTiersConfig),
}

impl Default for TieredTraversalConfig {
    fn default() -> Self {
        TieredTraversalConfig::AscendingTiers(AscendingTiersConfig::default())
    }
}

#[derive(Debug, typegen::TypeGen, PartialEq, Default)]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AscendingTiersConfig {
    pub tiers: Vec<AscendingTier>,
    /// If this is set, the traversal will stop at this tier
    /// and not traverse any further.
    pub max_tier: Option<usize>,
}

#[derive(Debug, typegen::TypeGen, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AscendingTier {
    pub name: TierName,
    pub tags_that_transition_to_this_tier: Vec<Tag>,
}

impl AscendingTiersConfig {
    pub fn tier_idx_to_name(&self, tier_idx: usize) -> Result<&str> {
        Ok(self
            .tiers
            .get(tier_idx)
            .with_context(|| {
                format!(
                    "Tier index {} out of bounds for tiers: {:?}",
                    tier_idx, self.tiers
                )
            })?
            .name
            .as_str())
    }

    pub fn make_tag_to_tier_idx_map(&self) -> BTreeMap<Tag, usize> {
        let mut tag_to_tier = BTreeMap::new();

        for (tier_idx, tier) in self.tiers.iter().enumerate() {
            for tag in &tier.tags_that_transition_to_this_tier {
                tag_to_tier.insert(tag.clone(), tier_idx);
            }
        }
        tag_to_tier
    }

    pub fn assign_tiers(
        &self,
        array_graph: &mut ArrayGraph,
        entry_points: &[NodeIDX],
    ) -> Result<()> {
        for node_idx in (0..array_graph.nodes_len()).map(NodeIDX::from) {
            // reset any tiers that we had assigned before
            array_graph.node_flags[node_idx].remove(NodeFlags::ALL_TIERS);
        }

        for next in array_graph
            .edges_forward
            .dfs_tiered_configured(&self.tiers, entry_points)?
        {
            let (node_idx, tier_idx) = next?;
            let tier_flags = tier_idx_to_flags(tier_idx)?;
            let node_tier_flags = NodeFlags::from_bits(tier_flags)
                .with_context(|| format!("Invalid tier flags: {tier_flags:#b}"))?;
            array_graph.node_flags[node_idx].insert(node_tier_flags);
        }

        Ok(())
    }
}

pub struct TieredTraversalIter<'a> {
    offset_graph: &'a OffsetGraph,
    current_tier: usize,
    visited: HashSet<NodeIDX>,
    stacks: [Vec<NodeIDX>; 4],
    tiers: Vec<AscendingTier>,
}

impl<'a> TieredTraversalIter<'a> {
    pub fn new(
        offset_graph: &'a OffsetGraph,
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
    ) -> Self {
        let stacks = [entry_points.to_vec(), Vec::new(), Vec::new(), Vec::new()];

        TieredTraversalIter {
            offset_graph,
            current_tier: 0,
            visited: HashSet::new(),
            stacks,
            tiers: tiers.to_vec(),
        }
    }

    fn try_next(&mut self) -> Result<Option<(NodeIDX, TierIDX)>> {
        if self.current_tier >= self.tiers.len() {
            return Ok(None);
        }

        if self.stacks[self.current_tier].is_empty() {
            // move to the next tier
            self.current_tier += 1;
            return self.try_next();
        }

        while let Some(node_idx) = self.stacks[self.current_tier].pop() {
            if self.visited.contains(&node_idx) {
                continue;
            }
            self.visited.insert(node_idx);

            for edge in self.offset_graph.edges_configured(node_idx) {
                if self.visited.contains(&edge.points_to) {
                    continue;
                }
                let transition_to_tier_idx = edge
                    .flags
                    .transitions_to_tier_idx()
                    .unwrap_or(self.current_tier);

                // we can only transition up, not down
                let child_tier = std::cmp::max(transition_to_tier_idx, self.current_tier);
                self.stacks[child_tier].push(edge.points_to);
            }

            return Ok(Some((node_idx, self.current_tier)));
        }

        self.current_tier += 1;
        self.try_next()
    }
}

impl Iterator for TieredTraversalIter<'_> {
    type Item = Result<(NodeIDX, TierIDX)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.try_next().transpose()
    }
}

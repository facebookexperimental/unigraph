use std::collections::BTreeMap;
use std::collections::HashSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::types::NodeIDX;
use crate::types::Tag;
use crate::types::TierName;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::EdgeFlags;

/// Configuration for tiered traversal, which allows traversing the graph in tiers.
/// Specific use case for this is JavaScript loading tiers. E.g. initial payload vs.
/// lazyloaded JS.
/// When we traverse the graph we look at the tagged edges. If the edge has a tag
/// we look at the node's current tier and then we look at the new tier this node
/// is supposed to transition to and record that.
#[derive(ts_rs::TS, Debug)]
#[ts(export)]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum TieredTraversalConfig {
    // Simple ascending tiers configuration.
    // This is specifically used for JS loading tiers.
    // Certain tagged edges will transition from one tier to another.
    // We can only transition up, not down. e.g., once you LazyLoad (second tier)
    // a JS module, everything past that tier will be considered lazyloaded
    // You can't go back to the initial tier.
    AscendingTiers(AscendingTiersConfig),
}

#[derive(ts_rs::TS, Debug)]
#[ts(export)]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AscendingTiersConfig {
    pub tiers: Vec<AscendingTier>,
    /// If this is set, the traversal will stop at this tier
    /// and not traverse any further.
    pub max_tier: Option<usize>,
}

#[derive(ts_rs::TS, Debug)]
#[ts(export)]
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
        anyhow::ensure!(self.tiers.len() <= 8, "Maximum of 8 tiers supported");

        let mut stacks: [Vec<NodeIDX>; 8] = Default::default();
        stacks[0] = entry_points.to_vec();
        let mut visited = HashSet::new();

        for node_idx in (0..array_graph.nodes_len()).map(NodeIDX::from) {
            // reset any tiers that we had assigned before
            array_graph.node_flags[node_idx].remove(NodeFlags::ALL_TIERS);
        }

        for tier_idx in 0..=3 {
            let tier_flag = match tier_idx {
                0 => NodeFlags::TIER_IDX_0,
                1 => NodeFlags::TIER_IDX_1,
                2 => NodeFlags::TIER_IDX_2,
                3 => NodeFlags::TIER_IDX_3,
                _ => anyhow::bail!("Invalid tier index: {}", tier_idx),
            };

            while let Some(node_idx) = stacks[tier_idx].pop() {
                if visited.contains(&node_idx) {
                    continue;
                }
                visited.insert(node_idx);
                array_graph.node_flags[node_idx].insert(tier_flag);

                for edge in array_graph.edges_forward.edges(node_idx) {
                    if edge.flags.contains(EdgeFlags::EXCLUDED) {
                        continue;
                    }
                    if visited.contains(&edge.points_to) {
                        continue;
                    }
                    let transition_to_tier_idx =
                        edge.flags.transitions_to_tier_idx().unwrap_or(tier_idx);

                    // we can only transition up, not down
                    let child_tier = std::cmp::max(transition_to_tier_idx, tier_idx);
                    stacks[child_tier].push(edge.points_to);
                }
            }
        }

        Ok(())
    }
}

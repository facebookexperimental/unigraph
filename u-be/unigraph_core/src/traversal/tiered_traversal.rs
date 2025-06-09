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
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;

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

    pub fn assign_tiers(
        &self,
        array_graph: &mut ArrayGraph,
        entry_points: &[NodeIDX],
    ) -> Result<()> {
        anyhow::ensure!(self.tiers.len() <= 8, "Maximum of 8 tiers supported");

        let mut tag_to_tier = BTreeMap::new();

        for (tier_idx, tier) in self.tiers.iter().enumerate() {
            for tag in &tier.tags_that_transition_to_this_tier {
                tag_to_tier.insert(tag.clone(), tier_idx);
            }
        }

        let mut stacks: [Vec<NodeIDX>; 8] = Default::default();
        stacks[0] = entry_points.to_vec();
        let mut visited = HashSet::new();

        for node_idx in (0..array_graph.nodes_len()).map(NodeIDX::from) {
            // reset any tiers that we had assigned before
            array_graph.node_flags[node_idx].remove(NodeFlags::ALL_TIERS);
        }

        for tier_idx in 0..=7 {
            let tier_flag = match tier_idx {
                0 => NodeFlags::TIER_0,
                1 => NodeFlags::TIER_1,
                2 => NodeFlags::TIER_2,
                3 => NodeFlags::TIER_3,
                4 => NodeFlags::TIER_4,
                5 => NodeFlags::TIER_5,
                6 => NodeFlags::TIER_6,
                7 => NodeFlags::TIER_7,
                _ => anyhow::bail!("Invalid tier index: {}", tier_idx),
            };

            while let Some(node_idx) = stacks[tier_idx].pop() {
                if visited.contains(&node_idx) {
                    continue;
                }
                visited.insert(node_idx);
                array_graph.node_flags[node_idx].insert(tier_flag);

                for (edge, metadata) in array_graph.edges_forward.edges_with_metadata(node_idx) {
                    if edge.flags.contains(EdgeFlags::EXCLUDED) {
                        continue;
                    }
                    if visited.contains(&edge.points_to) {
                        continue;
                    }
                    let transition_to_tier =
                        if let NonDirectedEdgeMetadata::Tagged { tag } = metadata {
                            tag_to_tier.get(tag).copied().unwrap_or(tier_idx)
                        } else {
                            tier_idx
                        };
                    // we can only transition up, not down
                    let child_tier = std::cmp::max(transition_to_tier, tier_idx);
                    stacks[child_tier].push(edge.points_to);
                }
            }
        }

        Ok(())
    }
}

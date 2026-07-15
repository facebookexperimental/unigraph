// Copyright (c) Meta Platforms, Inc. and affiliates.
use std::collections::BTreeMap;
use std::collections::HashSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::types::DynamicTypeKey;
use crate::types::NodeIDX;
use crate::types::Tag;
use crate::types::TierIDX;
use crate::types::TierName;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::EdgeOverrides;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::array_graph::offset_graph::edge_overrides::edge_should_be_followed;
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
    /// A tagged edge with any of these tags bumps its target node to this tier.
    pub tags_that_transition_to_this_tier: Vec<Tag>,
    /// A dynamic edge with any of these `DynamicTypeKey`s (e.g. `"rc:gk"`) bumps
    /// its target node to this tier — the dynamic-edge analog of
    /// `tags_that_transition_to_this_tier`. Defaulted so older serialized graphs
    /// (which predate this field) still deserialize.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dynamic_type_keys_that_transition_to_this_tier: Vec<DynamicTypeKey>,
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

    pub fn make_dynamic_type_key_to_tier_idx_map(&self) -> BTreeMap<DynamicTypeKey, usize> {
        let mut dynamic_type_key_to_tier = BTreeMap::new();

        for (tier_idx, tier) in self.tiers.iter().enumerate() {
            for dynamic_type_key in &tier.dynamic_type_keys_that_transition_to_this_tier {
                dynamic_type_key_to_tier.insert(dynamic_type_key.clone(), tier_idx);
            }
        }
        dynamic_type_key_to_tier
    }

    pub fn assign_tiers(
        &self,
        array_graph: &mut ArrayGraph,
        entry_points: &[NodeIDX],
    ) -> Result<()> {
        for node_idx in (0..array_graph.nodes_len()).map(NodeIDX::from) {
            // reset any tiers that we had assigned before
            array_graph.runtime.node_flags[node_idx].remove(NodeFlags::ALL_TIERS);
        }

        // Collect tier assignments first to avoid borrow conflict
        // (DFS borrows edge_flags immutably, we need to mutate node_flags)
        let tier_assignments: Vec<(NodeIDX, usize)> = {
            let iter = TieredTraversalIter::new(
                &array_graph.data.edges.edges,
                &array_graph.runtime.edge_flags,
                &array_graph.data.edges.edge_offsets,
                &self.tiers,
                entry_points,
            );
            iter.collect::<Result<Vec<_>>>()?
        };

        for (node_idx, tier_idx) in tier_assignments {
            let tier_flags = tier_idx_to_flags(tier_idx)?;
            let node_tier_flags = NodeFlags::from_bits(tier_flags)
                .with_context(|| format!("Invalid tier flags: {tier_flags:#b}"))?;
            array_graph.runtime.node_flags[node_idx].insert(node_tier_flags);
        }

        Ok(())
    }
}

pub struct TieredTraversalIter<'a> {
    targets: &'a [NodeIDX],
    flags: &'a [EdgeFlags],
    edge_offsets: &'a [usize],
    overrides: Option<&'a EdgeOverrides>,
    current_tier: usize,
    visited: HashSet<NodeIDX>,
    stacks: [Vec<NodeIDX>; 4],
    tiers: Vec<AscendingTier>,
}

impl<'a> TieredTraversalIter<'a> {
    pub fn new(
        targets: &'a [NodeIDX],
        flags: &'a [EdgeFlags],
        edge_offsets: &'a [usize],
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
    ) -> Self {
        Self::new_with_overrides(targets, flags, edge_offsets, tiers, entry_points, None)
    }

    pub fn new_with_overrides(
        targets: &'a [NodeIDX],
        flags: &'a [EdgeFlags],
        edge_offsets: &'a [usize],
        tiers: &[AscendingTier],
        entry_points: &[NodeIDX],
        overrides: Option<&'a EdgeOverrides>,
    ) -> Self {
        let stacks = [entry_points.to_vec(), Vec::new(), Vec::new(), Vec::new()];

        TieredTraversalIter {
            targets,
            flags,
            edge_offsets,
            overrides,
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

            let parent_overrides = self.overrides.and_then(|o| o.for_parent(node_idx));
            let start = self.edge_offsets[node_idx];
            let end = self.edge_offsets[node_idx + 1];
            for i in start..end {
                let target = self.targets[i];
                let edge_flags = self.flags[i];

                if !edge_should_be_followed(parent_overrides, target, edge_flags) {
                    continue;
                }

                if self.visited.contains(&target) {
                    continue;
                }
                let transition_to_tier_idx = edge_flags
                    .transitions_to_tier_idx()
                    .unwrap_or(self.current_tier);

                // we can only transition up, not down
                let child_tier = std::cmp::max(transition_to_tier_idx, self.current_tier);
                self.stacks[child_tier].push(target);
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::types::array_graph::NodeFlags;

pub trait ArrayGraphTestTrait {
    fn print_nodes(&self) -> String;
}

impl ArrayGraphTestTrait for crate::ArrayGraph {
    fn print_nodes(&self) -> String {
        let mut result = String::new();
        for node_idx in self.node_idx_iter() {
            let node_name = self.idx_to_name(node_idx);
            let unreachable_str = if self.node_flags[node_idx].contains(NodeFlags::UNREACHABLE) {
                " [UNREACHABLE]"
            } else {
                ""
            };
            result.push_str(&format!("{node_name}{unreachable_str}\n"));
            if let Some(tier_idx) = self.node_tier_idx(node_idx) {
                let tier_name = &self.state.tiers[tier_idx].0;
                result.push_str(&format!("  Tier: {tier_name}\n"));
            }
        }

        result
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::AscendingTier;
use crate::AscendingTiersConfig;
use crate::Decision;
use crate::TieredTraversalConfig;
use crate::TraversalConfig;

pub trait TraversalConfigTestTrait {
    fn set_force_node(&mut self, node_name: &str, decision: bool) -> &mut Self;
    fn get_tier_config(&self) -> AscendingTiersConfig;
    fn with_tier_config(&mut self) -> &mut Self;
    fn with_max_tier_idx(&mut self, max_tier_idx: usize) -> &mut Self;
}

impl TraversalConfigTestTrait for TraversalConfig {
    fn set_force_node(&mut self, node_name: &str, decision: bool) -> &mut Self {
        let decision = if decision {
            Decision::include()
        } else {
            Decision::exclude()
        };
        let force_nodes = self.force_nodes.get_or_insert_default();
        force_nodes.insert(node_name.into(), decision);
        self
    }

    fn get_tier_config(&self) -> AscendingTiersConfig {
        match self.tiered_traversal.as_ref().unwrap() {
            TieredTraversalConfig::AscendingTiers(tiered_config) => tiered_config.clone(),
        }
    }

    fn with_tier_config(&mut self) -> &mut Self {
        let tiers = vec![
            AscendingTier {
                name: "T1".into(),
                tags_that_transition_to_this_tier: vec![],
            },
            AscendingTier {
                name: "T2".into(),
                tags_that_transition_to_this_tier: vec!["RDFD".into()],
            },
            AscendingTier {
                name: "T3".into(),
                tags_that_transition_to_this_tier: vec!["RD".into()],
            },
            AscendingTier {
                name: "T4".into(),
                tags_that_transition_to_this_tier: vec!["BL".into()],
            },
        ];
        let max_tier = None;

        self.tiered_traversal = Some(TieredTraversalConfig::AscendingTiers(
            AscendingTiersConfig { tiers, max_tier },
        ));
        self
    }

    fn with_max_tier_idx(&mut self, max_tier_idx: usize) -> &mut Self {
        if let Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)) =
            &mut self.tiered_traversal
        {
            ascending_tiers.max_tier = Some(max_tier_idx);
        } else {
            self.tiered_traversal = Some(TieredTraversalConfig::AscendingTiers(
                AscendingTiersConfig {
                    tiers: vec![],
                    max_tier: Some(max_tier_idx),
                },
            ));
        }
        self
    }
}

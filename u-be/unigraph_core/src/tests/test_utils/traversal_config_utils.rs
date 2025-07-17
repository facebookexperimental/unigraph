// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::Decision;
use crate::TraversalConfig;

pub trait TraversalConfigTestUtils {
    fn set_force_children_of(&mut self, node_name: &str, decision: bool) -> &mut Self;
}

impl TraversalConfigTestUtils for TraversalConfig {
    fn set_force_children_of(&mut self, node_name: &str, decision: bool) -> &mut Self {
        let decision = if decision {
            Decision::include()
        } else {
            Decision::exclude()
        };
        self.force_children_of.insert(node_name.into(), decision);
        self
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;
use crate::traversal::messages::IndexedMessages;
use crate::types::TierIDX;
use crate::types::TierName;

/// This is a stuct that groups array graph fields that contain
/// some mutable state that's usually reapplied/modified on certain events.
/// Usually when we apply a traversal config we would update fields here
/// to hold certain state that is useful, but neither derived nor permanent.
#[derive(Default)]
pub struct ArrayGraphState {
    pub traversal_config: Option<TraversalConfig>,

    /// The live, mutable graph settings. Seeded from the immutable
    /// `data.graph_settings` at construction, then updated in place by
    /// `ArrayGraph::set_graph_settings` — so runtime edits don't touch the
    /// shared `Arc` payload. Authoritative at runtime; not persisted (mirrors
    /// `traversal_config`).
    pub graph_settings: Option<GraphSettings>,

    pub indexed_messages: IndexedMessages,

    /// This is useful for easier access to tiers by tier_idx
    pub tiers: Vec<(TierName, TierIDX)>,
}

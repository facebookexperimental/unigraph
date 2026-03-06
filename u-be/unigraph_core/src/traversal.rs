// Copyright (c) Meta Platforms, Inc. and affiliates.

pub(crate) mod apply_to_array_graph;
pub mod messages;
pub(crate) mod reachable_subgraph;
pub mod tiered_traversal;

use std::collections::BTreeMap;

use tiered_traversal::TieredTraversalConfig;

use crate::ArrayGraph;
use crate::AscendingTiersConfig;
use crate::traversal::messages::Message;
use crate::traversal::messages::MessageID;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::NodeIDX;
use crate::types::NodeName;
use crate::types::Tag;
use crate::types::TagSetName;
use crate::types::TierIDX;
use crate::types::TierName;

#[derive(Clone, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize, Debug, typegen::TypeGen)]
pub struct Decision {
    pub include: bool,
    pub message_id: Option<MessageID>,
}

impl Decision {
    pub fn include() -> Self {
        Decision {
            include: true,
            message_id: None,
        }
    }

    pub fn exclude() -> Self {
        Decision {
            include: false,
            message_id: None,
        }
    }
}

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Default,
    Clone,
    typegen::TypeGen,
    PartialEq
)]
pub struct TraversalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_nodes: Option<BTreeMap<NodeName, Decision>>,

    /// From Node Name -> To Node Name -> Decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_edges: Option<BTreeMap<NodeName, BTreeMap<NodeName, Decision>>>,

    /// Only applied to tagged edges
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_tagged: Option<BTreeMap<Tag, Decision>>,
    /// These rules are ordered. The first one that matches will be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_predicates: Option<Vec<NodeLabelPredicate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_dynamic: Option<BTreeMap<DynamicTypeKey, DynamicTypeConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_traversal: Option<TieredTraversalConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<BTreeMap<MessageID, Message>>,
}

/// The version of TraversalConfig that is used for NodeIDX instead of string
/// names so we can use it in a context of an array graph.
pub struct TraversalConfigIDX {
    pub force_nodes: BTreeMap<NodeIDX, Decision>,
    pub force_edges: BTreeMap<NodeIDX, BTreeMap<NodeIDX, Decision>>,
    pub force_tagged: BTreeMap<Tag, Decision>,
    /// These rules are ordered. The first one that matches will be used.
    pub label_predicates: Vec<NodeLabelPredicate>,
    pub force_dynamic: BTreeMap<DynamicTypeKey, DynamicTypeConfig>,

    pub tiered_traversal: Option<TieredTraversalConfig>,

    pub messages: BTreeMap<MessageID, Message>,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    typegen::TypeGen,
    PartialEq
)]
pub struct DynamicTypeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branches: Option<DefaultBranches>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<BTreeMap<DynamicEdgeName, DynamicEdgeOverride>>,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    typegen::TypeGen,
    PartialEq
)]
pub enum DefaultBranches {
    Include(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    typegen::TypeGen,
    PartialEq
)]
pub struct DynamicEdgeOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branches: Option<DefaultBranches>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
}

/// These predicates are used to decide whether to follow an edge to a node based
/// on node's tag sets, which will contain some annotations about the node.
///
/// Specifically there are two concepts here:
///        @assert_value v1 v2 v3: if the tag set is present, we ONLY follow the edge
///          if the tagset contains a passed value (set globally). Otherwise we do not
///          follow the edge.
///        @disallow_value v1 v2 v3: if the tagset is present, we do NOT follow the edge
///          if the tagset contains a passed value (set globally). Otherwise we do follow
///          the edge (unless other predicates disallow it).
///
/// assuming current route is "homepage".
/// this produces these predicates:
///
/// [
///    { tag_set_name: "assert_route", tag_name: "homepage", contains: true, decision: { include: true } },
///    { tag_set_name: "assert_route", tag_name: "homepage", contains: false, decision: { include: false } },
///    { tag_set_name: "disallow_route", tag_name: "homepage", contains: true, decision: { include: false } },
///    { tag_set_name: "disallow_route", tag_name: "homepage", contains: false, decision: { include: true } },
/// ]
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    typegen::TypeGen,
    PartialEq
)]
pub struct NodeLabelPredicate {
    pub tag_set_name: TagSetName,
    pub tag_name: Tag,
    pub contains: bool,
    pub decision: Decision,
}

impl TraversalConfig {
    pub fn index(&self, array_graph: &ArrayGraph) -> TraversalConfigIDX {
        let mut force_nodes = BTreeMap::new();

        for (name, decision) in self.force_nodes.iter().flatten() {
            if let Some(idx) = array_graph.nodes.name_to_idx_log(name) {
                force_nodes.insert(idx, decision.clone());
            }
        }
        let mut force_edges = BTreeMap::new();
        for (from_node_name, decisions) in self.force_edges.iter().flatten() {
            if let Some(from_idx) = array_graph.nodes.name_to_idx_log(from_node_name) {
                let inner_map = force_edges.entry(from_idx).or_insert(BTreeMap::new());
                for (to_node_name, decision) in decisions {
                    if let Some(to_idx) = array_graph.nodes.name_to_idx_log(to_node_name) {
                        inner_map.insert(to_idx, decision.clone());
                    }
                }
            }
        }

        TraversalConfigIDX {
            force_nodes,
            force_edges,
            force_tagged: self.force_tagged.clone().unwrap_or_default(),
            force_dynamic: self.force_dynamic.clone().unwrap_or_default(),
            label_predicates: self.label_predicates.clone().unwrap_or_default(),
            tiered_traversal: self.tiered_traversal.clone(),
            messages: self.messages.clone().unwrap_or_default(),
        }
    }

    pub fn get_tiers(&self) -> Vec<(TierName, TierIDX)> {
        match self {
            TraversalConfig {
                tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)),
                ..
            } => ascending_tiers
                .tiers
                .iter()
                .enumerate()
                .map(|(tier_idx, tier)| (tier.name.clone(), tier_idx))
                .collect(),
            TraversalConfig {
                tiered_traversal: None,
                ..
            } => vec![],
        }
    }
}

impl TraversalConfigIDX {
    pub fn ascending_tiers(&self) -> Option<&AscendingTiersConfig> {
        match &self.tiered_traversal {
            Some(TieredTraversalConfig::AscendingTiers(config)) => Some(config),
            _ => None,
        }
    }
}

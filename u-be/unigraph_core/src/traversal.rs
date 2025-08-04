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
use crate::types::NodeIDX;
use crate::types::NodeName;
use crate::types::Tag;
use crate::types::TagSetName;
use crate::types::TierIDX;
use crate::types::TierName;

#[derive(Clone)]
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

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, typegen::TypeGen)]
pub struct TraversalConfig {
    pub force_nodes: BTreeMap<NodeName, Decision>,
    /// From Node Name -> To Node Name -> Decision
    pub force_edges: BTreeMap<NodeName, BTreeMap<NodeName, Decision>>,

    /// This will force all nodes that are children of the given node.
    /// This is useful for cases where you want to exclude all imports
    /// of a specific node (like `MySharedInfraModules.js`) with a single
    /// config.
    #[serde(default)]
    pub force_children_of: BTreeMap<NodeName, Decision>,

    /// Only applied to tagged edges
    pub force_tagged: BTreeMap<Tag, Decision>,
    /// These rules are ordered. The first one that matches will be used.
    pub tag_sets: Vec<NodeTagSetsPredicate>,
    /// These rules are ordered. The first one that matches will be used.
    pub force_dynamic: Vec<ForceDynamic>,

    pub tiered_traversal: Option<TieredTraversalConfig>,

    pub messages: Option<BTreeMap<MessageID, Message>>,
}

/// The version of TraversalConfig that is used for NodeIDX instead of string
/// names so we can use it in a context of an array graph.
pub struct TraversalConfigIDX {
    pub force_nodes: BTreeMap<NodeIDX, Decision>,
    pub force_edges: BTreeMap<NodeIDX, BTreeMap<NodeIDX, Decision>>,
    pub force_children_of: BTreeMap<NodeIDX, Decision>,
    pub force_tagged: BTreeMap<Tag, Decision>,
    /// These rules are ordered. The first one that matches will be used.
    pub tag_sets: Vec<NodeTagSetsPredicate>,
    /// These rules are ordered. The first one that matches will be used.
    pub force_dynamic: Vec<ForceDynamicIDX>,

    pub tiered_traversal: Option<TieredTraversalConfig>,

    pub messages: BTreeMap<MessageID, Message>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, typegen::TypeGen)]
pub struct ForceDynamic {
    pub from_node: Option<NodeName>,
    pub match_properties: BTreeMap<String, String>,
    pub branch: Option<String>,
    pub decision: Decision,
}
#[derive(Debug)]
pub struct ForceDynamicIDX {
    pub from_node: Option<NodeIDX>,
    pub match_properties: BTreeMap<String, String>,
    pub branch: Option<String>,
    pub decision: Decision,
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
#[derive(serde::Serialize, serde::Deserialize, Clone, typegen::TypeGen)]
pub struct NodeTagSetsPredicate {
    pub tag_set_name: TagSetName,
    pub tag_name: Tag,
    pub contains: bool,
    pub decision: Decision,
}

impl TraversalConfig {
    pub fn index(&self, array_graph: &ArrayGraph) -> TraversalConfigIDX {
        let mut force_nodes = BTreeMap::new();

        for (name, decision) in &self.force_nodes {
            if let Some(idx) = array_graph.nodes.name_to_idx_log(name) {
                force_nodes.insert(idx, decision.clone());
            }
        }
        let mut force_edges = BTreeMap::new();
        for (from_node_name, decisions) in &self.force_edges {
            if let Some(from_idx) = array_graph.nodes.name_to_idx_log(from_node_name) {
                let inner_map = force_edges.entry(from_idx).or_insert(BTreeMap::new());
                for (to_node_name, decision) in decisions {
                    if let Some(to_idx) = array_graph.nodes.name_to_idx_log(to_node_name) {
                        inner_map.insert(to_idx, decision.clone());
                    }
                }
            }
        }

        let force_dynamic = self
            .force_dynamic
            .iter()
            .map(|dynamic| ForceDynamicIDX {
                from_node: dynamic
                    .from_node
                    .as_ref()
                    .and_then(|name| array_graph.nodes.name_to_idx_log(name)),
                match_properties: dynamic.match_properties.clone(),
                branch: dynamic.branch.clone(),
                decision: dynamic.decision.clone(),
            })
            .collect();

        let force_children_of = self
            .force_children_of
            .iter()
            .filter_map(|(name, decision)| {
                array_graph
                    .nodes
                    .name_to_idx_log(name)
                    .map(|idx| (idx, decision.clone()))
            })
            .collect();

        TraversalConfigIDX {
            force_nodes,
            force_edges,
            force_children_of,
            force_tagged: self.force_tagged.clone(),
            force_dynamic,
            tag_sets: self.tag_sets.clone(),
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

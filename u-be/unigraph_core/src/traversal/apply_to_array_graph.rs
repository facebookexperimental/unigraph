// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::Decision;
use crate::NodeIDX;
use crate::NodeTagSetsPredicate;
use crate::TraversalConfig;
use crate::traversal::ForceDynamicIDX;
use crate::traversal::TraversalConfigIDX;
use crate::traversal::messages::BuiltInMessages;
use crate::traversal::messages::IndexedMessages;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::Tag;
use crate::types::TierName;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata;

/// This function will take an `ArrayGraph` and a `TraversalConfig`, and apply the traversal configuration to the graph.
/// which will include figuring out which edges/nodes to follow, which edges/nodes to exclude, assign tiers if
/// it has a tiered traversal and add all node metadata
pub fn apply_traversal_config_to_array_graph(
    ag: &mut ArrayGraph,
    traversal_config: TraversalConfig,
) -> Result<()> {
    let entry_points = ag.determine_entrypoints();
    let indexed_config = traversal_config.index(ag);
    let m = IndexedMessages::new_with_builtin(&indexed_config.messages);

    let tag_to_tier = indexed_config
        .ascending_tiers()
        .map(|c| c.make_tag_to_tier_idx_map());

    let TraversalConfigIDX {
        force_nodes,
        force_edges,
        force_tagged,
        tag_sets,
        force_dynamic,
        tiered_traversal,
        messages: _,
    } = &indexed_config;

    let exclude_tags = exclude_tags_for_tier_above_the_max(tiered_traversal);

    for (parent_idx, edge, md) in ag.edges_forward.iter_edges_mut() {
        // we need to start fresh and make sure all edges that were previously excluded
        // are reset.
        edge.flags.include();

        match_dynamic_edges(force_dynamic, parent_idx, edge, md, &m)?;
        match_tagged(force_tagged, &exclude_tags, edge, md, &tag_to_tier, &m)?;
        if let Some(tag_sets_for_node) = ag.tag_sets.get(&edge.points_to) {
            match_tag_sets(tag_sets, edge, tag_sets_for_node, &m)?;
        }
        match_force_edges(force_edges, parent_idx, edge, &m)?;
        match_force_nodes(force_nodes, edge, &m)?;
    }

    apply_tiers(ag, &indexed_config, &entry_points)?;

    apply_node_reachability(ag, entry_points);

    ag.derived_state = ArrayGraphDerivedState::from_forward_edges(&ag.edges_forward);
    ag.state = ArrayGraphState {
        tiers: traversal_config.get_tiers(),
        traversal_config: Some(traversal_config),
        indexed_messages: m,
    };
    Ok(())
}

/// If we have `max_tier` set we can look at what tags these tiers use to transition to
/// greater tiers and exclude those tags
fn exclude_tags_for_tier_above_the_max(
    tiered_traversal: &Option<TieredTraversalConfig>,
) -> Option<BTreeSet<TierName>> {
    if let Some(TieredTraversalConfig::AscendingTiers(config)) = tiered_traversal {
        if let Some(max_tier) = config.max_tier {
            let exclude_tags = config
                .tiers
                .iter()
                .enumerate()
                .filter_map(|(tier_idx, tier)| {
                    if tier_idx > max_tier {
                        Some(tier.tags_that_transition_to_this_tier.clone())
                    } else {
                        None
                    }
                })
                .flatten()
                .collect::<BTreeSet<_>>();

            if !exclude_tags.is_empty() {
                return Some(exclude_tags);
            }
        }
    }

    None
}

fn apply_node_reachability(ag: &mut ArrayGraph, entry_points: Vec<NodeIDX>) {
    for node_idx in ag.node_idx_iter() {
        // first mark all nodes as unreachable
        ag.node_flags[node_idx].insert(NodeFlags::UNREACHABLE);
    }

    for node_idx in ag.edges_forward.dfs_configured(&entry_points) {
        // whatever is reachable from a configured DFS we can mark as reachable
        ag.node_flags[node_idx].remove(NodeFlags::UNREACHABLE);
    }
}

fn apply_tiers(
    ag: &mut ArrayGraph,
    indexed_config: &TraversalConfigIDX,
    entry_points: &[NodeIDX],
) -> Result<(), anyhow::Error> {
    match &indexed_config.tiered_traversal {
        Some(TieredTraversalConfig::AscendingTiers(tiers_config)) => {
            tiers_config.assign_tiers(ag, entry_points)?;
        }
        None => {
            // No tiered traversal, do nothing
        }
    }

    Ok(())
}

fn match_force_nodes(
    force_nodes: &BTreeMap<NodeIDX, Decision>,
    edge: &mut Edge,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    if let Some(decision) = force_nodes.get(&edge.points_to) {
        match decision.include {
            true => {
                edge.flags
                    .include_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::NODE_FORCE_INCLUDED_ID,
                    ))?;
            }
            false => {
                edge.flags
                    .exclude_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::NODE_FORCE_EXCLUDED_ID,
                    ))?;
            }
        }
    }
    Ok(())
}

fn match_force_edges(
    force_edges: &BTreeMap<NodeIDX, BTreeMap<NodeIDX, Decision>>,
    parent_idx: crate::NodeIDX,
    edge: &mut Edge,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    #[allow(clippy::collapsible_if)]
    if let Some(force_to) = force_edges.get(&parent_idx) {
        if let Some(decision) = force_to.get(&edge.points_to) {
            match decision.include {
                true => edge
                    .flags
                    .include_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::EDGE_FORCE_INCLUDED_ID,
                    ))?,
                false => {
                    edge.flags
                        .exclude_with_message(indexed_messages.get_or_default(
                            &decision.message_id,
                            BuiltInMessages::EDGE_FORCE_EXCLUDED_ID,
                        ))?;
                }
            }
        }
    }

    Ok(())
}

fn match_tag_sets(
    tag_sets: &[NodeTagSetsPredicate],
    edge: &mut Edge,
    tag_sets_for_node: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    for tag_set_predicate in tag_sets {
        #[allow(clippy::collapsible_if)]
        if let Some(tags) = tag_sets_for_node.get(&tag_set_predicate.tag_set_name) {
            if tags.contains(&tag_set_predicate.tag_name) == tag_set_predicate.contains {
                match tag_set_predicate.decision.include {
                    true => edge
                        .flags
                        .include_with_message(indexed_messages.get_or_default(
                            &tag_set_predicate.decision.message_id,
                            BuiltInMessages::FORCE_TAG_SETS_INCLUDED_ID,
                        ))?,
                    false => edge
                        .flags
                        .exclude_with_message(indexed_messages.get_or_default(
                            &tag_set_predicate.decision.message_id,
                            BuiltInMessages::FORCE_TAG_SETS_EXCLUDED_ID,
                        ))?,
                }
            }
        }
    }

    Ok(())
}

fn match_dynamic_edges(
    force_dynamic: &[ForceDynamicIDX],
    parent_idx: crate::NodeIDX,
    edge: &mut Edge,
    metadata: &mut NonDirectedEdgeMetadata,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    if let NonDirectedEdgeMetadata::Dynamic { properties, branch } = metadata {
        if let Some(decision) = match_dynamic_edge(force_dynamic, properties, parent_idx, branch) {
            match decision.include {
                true => edge
                    .flags
                    .include_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::FORCE_DYNAMIC_INCLUDED_ID,
                    ))?,
                false => edge
                    .flags
                    .exclude_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::FORCE_DYNAMIC_EXCLUDED_ID,
                    ))?,
            }
        }
    }

    Ok(())
}
fn match_tagged(
    force_tagged: &BTreeMap<Tag, Decision>,
    exclude_tags: &Option<BTreeSet<Tag>>,
    edge: &mut Edge,
    metadata: &mut NonDirectedEdgeMetadata,
    tag_to_tier: &Option<BTreeMap<Tag, usize>>,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    #[allow(clippy::collapsible_if)]
    if let NonDirectedEdgeMetadata::Tagged { tag } = metadata {
        if let Some(exclude_tags) = exclude_tags {
            if exclude_tags.contains(tag) {
                edge.flags.exclude_with_message(
                    indexed_messages
                        .get(BuiltInMessages::EDGE_EXCLUDED_BECAUSE_GREATER_THAN_MAX_TIER_ID),
                )?;
                return Ok(());
            }
        }

        if let Some(decision) = force_tagged.get(tag) {
            match decision.include {
                true => edge
                    .flags
                    .include_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::FORCE_TAGGED_INCLUDED_ID,
                    ))?,
                false => edge
                    .flags
                    .exclude_with_message(indexed_messages.get_or_default(
                        &decision.message_id,
                        BuiltInMessages::FORCE_TAGGED_EXCLUDED_ID,
                    ))?,
            }
        }

        if let Some(tag_to_tier) = tag_to_tier {
            if let Some(tier_idx) = tag_to_tier.get(tag).copied() {
                edge.flags.set_transitions_to_tier_idx(tier_idx)?;
            }
        }
    }
    Ok(())
}

pub fn match_dynamic_edge(
    force_dynamic: &[ForceDynamicIDX],
    properties: &BTreeMap<String, String>,
    from_node: NodeIDX,
    branch: &str,
) -> Option<Decision> {
    for dynamic_predicate in force_dynamic {
        if let Some(from_node_idx_predicate) = dynamic_predicate.from_node {
            // if parent node is specified and it does not match the current node,
            // skip this whole predicate
            if from_node_idx_predicate != from_node {
                continue;
            }
        }

        if let Some(branch_predicate) = &dynamic_predicate.branch {
            // if branch is specified and it does not match the current branch,
            // skip this whole predicate
            if branch_predicate != branch {
                continue;
            }
        }

        if dynamic_predicate
            .match_properties
            .iter()
            .all(|(key, value)| properties.get(key) == Some(value))
        {
            return Some(dynamic_predicate.decision.clone());
        }
    }

    None
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::ArrayGraph;
use crate::AscendingTiersConfig;
use crate::NodeIDX;
use crate::TraversalConfig;
use crate::traversal::TraversalConfigIDX;
use crate::traversal::messages::BuiltInMessages;
use crate::traversal::messages::IndexedMessages;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::Tag;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
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

    for (parent_idx, edge, metadata) in ag.edges_forward.iter_edges_mut() {
        // we need to start fresh and make sure all edges that were previously excluded
        // are reset.
        edge.flags.include();

        match_dynamic_edges(&indexed_config, parent_idx, edge, metadata);
        match_tagged(&indexed_config, edge, metadata, &tag_to_tier)?;
        if let Some(tag_sets_for_node) = ag.tag_sets.get(&edge.points_to) {
            match_tag_sets(&indexed_config, edge, tag_sets_for_node);
        }
        match_force_edges(&indexed_config, parent_idx, edge);
        match_force_nodes(&indexed_config, edge, &m)?;
    }

    apply_tiers(ag, &indexed_config, &entry_points)?;
    exclude_edges_below_max_tier(ag, &indexed_config)?;

    apply_node_reachability(ag, entry_points);

    ag.state = ArrayGraphState {
        tiers: traversal_config.get_tiers(),
        traversal_config: Some(traversal_config),
        indexed_messages: m,
    };
    Ok(())
}

/// If max tier set, do another traversal of all edges and exclude any edges
/// that point to a node with a tier above the max tier.
fn exclude_edges_below_max_tier(
    ag: &mut ArrayGraph,
    indexed_config: &TraversalConfigIDX,
) -> Result<()> {
    if let Some(AscendingTiersConfig {
        max_tier: Some(max_tier),
        ..
    }) = indexed_config.ascending_tiers()
    {
        for (_from, edge, _metadata) in ag.edges_forward.iter_edges_mut() {
            #[allow(clippy::collapsible_if)]
            if let Some(points_to_tier) = ag.node_flags[edge.points_to].tier_idx() {
                if points_to_tier > *max_tier {
                    edge.flags.exclude();
                }
            }
        }
    }

    Ok(())
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
    indexed_config: &super::TraversalConfigIDX,
    edge: &mut crate::types::array_graph::offset_graph::Edge,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    if let Some(decision) = indexed_config.force_nodes.get(&edge.points_to) {
        match decision.include {
            true => edge.flags.include(),
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
    indexed_config: &super::TraversalConfigIDX,
    parent_idx: crate::NodeIDX,
    edge: &mut crate::types::array_graph::offset_graph::Edge,
) {
    #[allow(clippy::collapsible_if)]
    if let Some(force_to) = indexed_config.force_edges.get(&parent_idx) {
        if let Some(decision) = force_to.get(&edge.points_to) {
            match decision.include {
                true => edge.flags.include(),
                false => edge.flags.exclude(),
            }
        }
    }
}

fn match_tag_sets(
    indexed_config: &super::TraversalConfigIDX,
    edge: &mut crate::types::array_graph::offset_graph::Edge,
    tag_sets_for_node: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    for tag_set_predicate in &indexed_config.tag_sets {
        #[allow(clippy::collapsible_if)]
        if let Some(tags) = tag_sets_for_node.get(&tag_set_predicate.tag_set_name) {
            if tags.contains(&tag_set_predicate.tag_name) == tag_set_predicate.contains {
                match tag_set_predicate.decision.include {
                    true => edge.flags.include(),
                    false => edge.flags.exclude(),
                }
            }
        }
    }
}

fn match_dynamic_edges(
    indexed_config: &super::TraversalConfigIDX,
    parent_idx: crate::NodeIDX,
    edge: &mut crate::types::array_graph::offset_graph::Edge,
    metadata: &mut NonDirectedEdgeMetadata,
) {
    if let NonDirectedEdgeMetadata::Dynamic { properties, branch } = metadata {
        let decision = indexed_config.match_dynamic_edge(properties, parent_idx, branch);
        match decision.include {
            true => edge.flags.include(),
            false => edge.flags.exclude(),
        }
    }
}
fn match_tagged(
    indexed_config: &super::TraversalConfigIDX,
    edge: &mut crate::types::array_graph::offset_graph::Edge,
    metadata: &mut NonDirectedEdgeMetadata,
    tag_to_tier: &Option<BTreeMap<Tag, usize>>,
) -> Result<()> {
    #[allow(clippy::collapsible_if)]
    if let NonDirectedEdgeMetadata::Tagged { tag } = metadata {
        if let Some(decision) = indexed_config.force_tagged.get(tag) {
            match decision.include {
                true => edge.flags.include(),
                false => edge.flags.exclude(),
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::TraversalConfig;
use crate::traversal::TraversalConfigIDX;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::offset_graph::EdgeFlags;
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

    for (parent_idx, edge, metadata) in ag.edges_forward.iter_edges_mut() {
        // we need to start fresh and make sure all edges that were previously excluded
        // are reset.
        edge.flags.remove(EdgeFlags::EXCLUDED);

        match_dynamic_edges(&indexed_config, parent_idx, edge, metadata);
        match_tagged(&indexed_config, edge, metadata);
        if let Some(tag_sets_for_node) = ag.tag_sets.get(&edge.points_to) {
            match_tag_sets(&indexed_config, edge, tag_sets_for_node);
        }
        match_force_edges(&indexed_config, parent_idx, edge);
        match_force_nodes(&indexed_config, edge);
    }

    apply_tiers(ag, &indexed_config, &entry_points)?;
    apply_node_reachability(ag, entry_points);
    ag.traversal_config = Some(traversal_config);
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
) {
    if let Some(decision) = indexed_config.force_nodes.get(&edge.points_to) {
        match decision.include {
            true => edge.flags.remove(EdgeFlags::EXCLUDED),
            false => edge.flags.insert(EdgeFlags::EXCLUDED),
        }
    }
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
                true => edge.flags.remove(EdgeFlags::EXCLUDED),
                false => edge.flags.insert(EdgeFlags::EXCLUDED),
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
                    true => edge.flags.remove(EdgeFlags::EXCLUDED),
                    false => edge.flags.insert(EdgeFlags::EXCLUDED),
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
            true => edge.flags.remove(EdgeFlags::EXCLUDED),
            false => edge.flags.insert(EdgeFlags::EXCLUDED),
        }
    }
}
fn match_tagged(
    indexed_config: &super::TraversalConfigIDX,
    edge: &mut crate::types::array_graph::offset_graph::Edge,
    metadata: &mut NonDirectedEdgeMetadata,
) {
    #[allow(clippy::collapsible_if)]
    if let NonDirectedEdgeMetadata::Tagged { tag } = metadata {
        if let Some(decision) = indexed_config.force_tagged.get(tag) {
            match decision.include {
                true => edge.flags.remove(EdgeFlags::EXCLUDED),
                false => edge.flags.insert(EdgeFlags::EXCLUDED),
            }
        }
    }
}

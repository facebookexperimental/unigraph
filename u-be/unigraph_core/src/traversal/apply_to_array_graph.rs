// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::Decision;
use crate::EdgeMeta;
use crate::NodeIDX;
use crate::NodeLabelPredicate;
use crate::TraversalConfig;
use crate::traversal::DefaultBranches;
use crate::traversal::DynamicTypeConfig;
use crate::traversal::TraversalConfigIDX;
use crate::traversal::messages::BuiltInMessages;
use crate::traversal::messages::IndexedMessages;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::DynamicTypeKey;
use crate::types::EdgeIDX;
use crate::types::LabelName;
use crate::types::LabelValue;
use crate::types::Tag;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
use crate::types::array_graph::offset_graph::Edge;

/// Applies the *global*, entry-point-independent part of a `TraversalConfig` to the graph:
/// figures out which edges/nodes to follow vs. exclude and stamps tier-*transition* flags on
/// edges (which tag bumps a node to which tier). This is a pure function of the config plus the
/// graph's tags/structure — it does NOT assign per-node tiers or compute node reachability.
///
/// For those entry-point-dependent outputs call [`apply_entry_point_state`] afterwards (or use
/// the [`apply_traversal_config_and_entry_points`] wrapper for the common "do both" case).
pub fn apply_traversal_config_to_array_graph(
    ag: &mut ArrayGraph,
    traversal_config: TraversalConfig,
) -> Result<()> {
    let indexed_config = traversal_config.index(ag);
    let m = IndexedMessages::new_with_builtin(&indexed_config.messages);

    let tag_to_tier = indexed_config
        .ascending_tiers()
        .map(|c| c.make_tag_to_tier_idx_map());
    let dynamic_type_key_to_tier = indexed_config
        .ascending_tiers()
        .map(|c| c.make_dynamic_type_key_to_tier_idx_map());

    let TraversalConfigIDX {
        force_nodes,
        force_edges,
        force_tagged,
        label_predicates,
        force_dynamic,
        tiered_traversal,
        messages: _,
    } = &indexed_config;

    let excluded_above_max = transitions_above_max_tier(tiered_traversal);
    let labels = &ag.data.node_metadata.labels;

    let node_count = ag.data.edges.edge_offsets.len() - 1;
    for node in 0..node_count {
        let parent_idx = NodeIDX::from(node);
        let start = ag.data.edges.edge_offsets[node];
        let end = ag.data.edges.edge_offsets[node + 1];
        for edge_i in start..end {
            let target = ag.data.edges.edges[edge_i];
            let edge_flags_ref = &mut ag.runtime.edge_flags[edge_i];

            // we need to start fresh and make sure all edges that were previously excluded
            // are reset.
            edge_flags_ref.include();

            // Look up metadata for this edge
            let meta = ag
                .data
                .edges
                .edge_metadata_map
                .get(&EdgeIDX::from(edge_i))
                .map(|&meta_idx| &ag.data.edges.edge_metadata[usize::from(meta_idx)]);

            let mut edge = Edge::new_with_flags(target, *edge_flags_ref);

            match_dynamic_edges(
                force_dynamic,
                excluded_above_max.as_ref().map(|e| &e.dynamic_type_keys),
                parent_idx,
                &mut edge,
                meta,
                &dynamic_type_key_to_tier,
                &m,
            )?;
            match_tagged(
                force_tagged,
                excluded_above_max.as_ref().map(|e| &e.tags),
                &mut edge,
                meta,
                &tag_to_tier,
                &m,
            )?;
            let labels_for_node = collect_labels_for_node(labels, edge.points_to);
            if !labels_for_node.is_empty() {
                match_label_predicates(label_predicates, &mut edge, &labels_for_node, &m)?;
            }
            match_force_edges(force_edges, parent_idx, &mut edge, &m)?;
            match_force_nodes(force_nodes, &mut edge, &m)?;

            // Write back the modified flags
            ag.runtime.edge_flags[edge_i] = edge.flags;
        }
    }

    ag.runtime.derived_state = ArrayGraphDerivedState::new();
    // Preserve the live graph settings — applying a traversal config replaces
    // the rest of the runtime state but must not reset settings.
    let graph_settings = ag.runtime.state.graph_settings.take();
    ag.runtime.state = ArrayGraphState {
        tiers: traversal_config.get_tiers(),
        traversal_config: Some(traversal_config),
        graph_settings,
        indexed_messages: m,
    };
    Ok(())
}

/// Computes the entry-point-*dependent* graph state: per-node tier assignments and node
/// reachability, both derived by walking from `entry_points`. Writes into `runtime.node_flags`.
///
/// Requires [`apply_traversal_config_to_array_graph`] to have run first: tier assignment walks the
/// edge tier-transition flags and reachability walks the edge inclusion flags that it stamps, and
/// the tiered config is read back from `runtime.state.traversal_config`.
pub fn apply_entry_point_state(ag: &mut ArrayGraph, entry_points: &[NodeIDX]) -> Result<()> {
    if let Some(traversal_config) = ag.runtime.state.traversal_config.clone() {
        let indexed_config = traversal_config.index(ag);
        apply_tiers(ag, &indexed_config, entry_points)?;
    }

    apply_node_reachability(ag, entry_points);

    // Reachability just changed, so the SCC/dominator/reverse caches are stale — drop them.
    ag.runtime.derived_state = ArrayGraphDerivedState::new();
    Ok(())
}

/// Convenience wrapper for the common "apply the config and resolve entry-point state" case:
/// applies the global config, then computes tiers + reachability from the graph's own entry points.
///
/// Entry points are determined *before* the config is applied so the result matches the pre-split
/// behavior (they reflect the pre-apply edge flags). Callers that want to scope by explicit roots
/// should instead call [`apply_traversal_config_to_array_graph`] then
/// [`apply_entry_point_state`] with their own roots.
pub fn apply_traversal_config_and_entry_points(
    ag: &mut ArrayGraph,
    traversal_config: TraversalConfig,
) -> Result<()> {
    let entry_points = ag.determine_entrypoints();
    apply_traversal_config_to_array_graph(ag, traversal_config)?;
    apply_entry_point_state(ag, &entry_points)
}

/// Tags and dynamic type keys whose transition target tier is above `max_tier`.
/// The edges carrying them must be excluded so traversal stops at `max_tier`.
#[derive(Default)]
struct TransitionsAboveMaxTier {
    tags: BTreeSet<Tag>,
    dynamic_type_keys: BTreeSet<DynamicTypeKey>,
}

/// If `max_tier` is set, collect the tags and dynamic type keys that transition to
/// tiers above it, so their edges can be excluded from traversal.
fn transitions_above_max_tier(
    tiered_traversal: &Option<TieredTraversalConfig>,
) -> Option<TransitionsAboveMaxTier> {
    if let Some(TieredTraversalConfig::AscendingTiers(config)) = tiered_traversal {
        if let Some(max_tier) = config.max_tier {
            let mut excluded = TransitionsAboveMaxTier::default();
            for (tier_idx, tier) in config.tiers.iter().enumerate() {
                if tier_idx > max_tier {
                    excluded
                        .tags
                        .extend(tier.tags_that_transition_to_this_tier.iter().cloned());
                    excluded.dynamic_type_keys.extend(
                        tier.dynamic_type_keys_that_transition_to_this_tier
                            .iter()
                            .cloned(),
                    );
                }
            }

            if !excluded.tags.is_empty() || !excluded.dynamic_type_keys.is_empty() {
                return Some(excluded);
            }
        }
    }

    None
}

fn apply_node_reachability(ag: &mut ArrayGraph, entry_points: &[NodeIDX]) {
    use crate::types::array_graph::offset_graph::DFSConfigured;

    for node_idx in ag.node_idx_iter() {
        // first mark all nodes as unreachable
        ag.runtime.node_flags[node_idx].insert(NodeFlags::UNREACHABLE);
    }

    // Use DFSConfigured directly with separate borrows to avoid borrow conflict
    // (DFS borrows edge_flags, we mutate node_flags — they're separate fields)
    let reachable: Vec<NodeIDX> = DFSConfigured::new(
        &ag.data.edges.edges,
        &ag.runtime.edge_flags,
        &ag.data.edges.edge_offsets,
        entry_points,
    )
    .collect();

    for node_idx in reachable {
        ag.runtime.node_flags[node_idx].remove(NodeFlags::UNREACHABLE);
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

fn match_label_predicates(
    label_predicates: &BTreeMap<String, NodeLabelPredicate>,
    edge: &mut Edge,
    labels_for_node: &BTreeMap<&str, &BTreeSet<String>>,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    for predicate in label_predicates.values() {
        #[allow(clippy::collapsible_if)]
        if let Some(values) = labels_for_node.get(predicate.label_name.as_str()) {
            if values.contains(&predicate.label_value) == predicate.contains {
                match predicate.decision.include {
                    true => edge
                        .flags
                        .include_with_message(indexed_messages.get_or_default(
                            &predicate.decision.message_id,
                            BuiltInMessages::FORCE_LABELS_INCLUDED_ID,
                        ))?,
                    false => edge
                        .flags
                        .exclude_with_message(indexed_messages.get_or_default(
                            &predicate.decision.message_id,
                            BuiltInMessages::FORCE_LABELS_EXCLUDED_ID,
                        ))?,
                }
            }
        }
    }

    Ok(())
}

/// Applies force_dynamic config to a dynamic edge.
///
/// Resolution for an edge with type_key + edge_name + branch:
///
///   1. Look up edge-specific override by (type_key, edge_name)
///   2. If the override has `branches` only (no `decision`) → apply the filter
///      to all branches (listed or not). This is the simple case.
///   3. If the override has BOTH `branches` and `decision` → apply the filter
///      only to branches explicitly listed in it. Unlisted branches (new/unknown
///      ones added after the TVC was built) fall back to `decision`.
///   4. If no override matched → fall back to type-level `default_branches`
///   5. If nothing matched → edge stays included (default)
fn match_dynamic_edges(
    force_dynamic: &BTreeMap<DynamicTypeKey, DynamicTypeConfig>,
    exclude_dynamic_type_keys: Option<&BTreeSet<DynamicTypeKey>>,
    _parent_idx: crate::NodeIDX,
    edge: &mut Edge,
    metadata: Option<&EdgeMeta>,
    dynamic_type_key_to_tier: &Option<BTreeMap<DynamicTypeKey, usize>>,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    if let Some(EdgeMeta::Dynamic {
        type_key,
        edge_name,
        branch,
        ..
    }) = metadata
    {
        // A dynamic edge whose type key transitions above `max_tier` is excluded
        // outright, mirroring the tagged-edge behavior in `match_tagged`.
        if let Some(exclude_dynamic_type_keys) = exclude_dynamic_type_keys {
            if exclude_dynamic_type_keys.contains(type_key) {
                edge.flags.exclude_with_message(
                    indexed_messages
                        .get(BuiltInMessages::EDGE_EXCLUDED_BECAUSE_GREATER_THAN_MAX_TIER_ID),
                )?;
                return Ok(());
            }
        }

        // Stamp the tier-transition bit from the edge's type key. This is
        // independent of the include/exclude decisions below (which touch
        // separate flag bits), so it applies regardless of `force_dynamic`.
        if let Some(dynamic_type_key_to_tier) = dynamic_type_key_to_tier {
            if let Some(tier_idx) = dynamic_type_key_to_tier.get(type_key).copied() {
                edge.flags.set_transitions_to_tier_idx(tier_idx)?;
            }
        }

        if let Some(type_config) = force_dynamic.get(type_key) {
            if let Some(overrides) = &type_config.overrides {
                if let Some(edge_override) = overrides.get(edge_name) {
                    if let Some(branches) = &edge_override.branches {
                        if edge_override.decision.is_none() || branch_is_listed(branch, branches) {
                            apply_branch_filter(edge, branch, branches, indexed_messages)?;
                            return Ok(());
                        }
                    }
                    if let Some(decision) = &edge_override.decision {
                        apply_dynamic_decision(edge, decision, indexed_messages)?;
                        return Ok(());
                    }
                }
            }
            if let Some(default_branches) = &type_config.default_branches {
                apply_branch_filter(edge, branch, default_branches, indexed_messages)?;
            }
        }
    }

    Ok(())
}

fn branch_is_listed(branch: &str, branches: &DefaultBranches) -> bool {
    match branches {
        DefaultBranches::Include(list) | DefaultBranches::Exclude(list) => {
            list.iter().any(|b| b == branch)
        }
    }
}

fn apply_dynamic_decision(
    edge: &mut Edge,
    decision: &Decision,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
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
    Ok(())
}

fn apply_branch_filter(
    edge: &mut Edge,
    branch: &str,
    branches: &DefaultBranches,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    let should_include = match branches {
        DefaultBranches::Include(list) => list.iter().any(|b| b == branch),
        DefaultBranches::Exclude(list) => !list.iter().any(|b| b == branch),
    };

    if should_include {
        edge.flags.include_with_message(
            indexed_messages.get(BuiltInMessages::FORCE_DYNAMIC_INCLUDED_ID),
        )?;
    } else {
        edge.flags.exclude_with_message(
            indexed_messages.get(BuiltInMessages::FORCE_DYNAMIC_EXCLUDED_ID),
        )?;
    }
    Ok(())
}

fn match_tagged(
    force_tagged: &BTreeMap<Tag, Decision>,
    exclude_tags: Option<&BTreeSet<Tag>>,
    edge: &mut Edge,
    metadata: Option<&EdgeMeta>,
    tag_to_tier: &Option<BTreeMap<Tag, usize>>,
    indexed_messages: &IndexedMessages,
) -> Result<()> {
    #[allow(clippy::collapsible_if)]
    if let Some(EdgeMeta::Tagged { tag }) = metadata {
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

/// Collect all labels for a specific node from the inverted labels index.
/// This is a free function (not a method on ArrayGraph) to avoid borrow conflicts
/// when used inside `iter_edges_mut()`.
fn collect_labels_for_node(
    labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    node_idx: NodeIDX,
) -> BTreeMap<&str, &BTreeSet<String>> {
    labels
        .iter()
        .filter_map(|(label_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|values| (label_name.as_str(), values))
        })
        .collect()
}

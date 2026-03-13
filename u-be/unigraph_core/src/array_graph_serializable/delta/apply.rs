// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
use unigraph_delta::Deltable;
use unigraph_delta::MapDelta;
use unigraph_delta::OptionDelta;

use super::MapGraphDelta;
use crate::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::ArrayGraphSerializableEdges;
use crate::ArrayGraphSerializableNodeMetadata;
use crate::NodeIDX;
use crate::remap_utils::RemapContext;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::LabelName;
use crate::types::LabelValue;
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::Tag;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;
use crate::types::map_graph::GraphNodeDelta;

/// Apply a single delta to a base graph, producing the resulting graph.
/// Consumes the base graph — if the delta is empty, the base is returned as-is
/// without copying.
pub fn apply_delta(
    base: ArrayGraphSerializable,
    delta: &MapGraphDelta,
) -> Result<ArrayGraphSerializable> {
    apply_deltas(base, std::slice::from_ref(delta))
}

/// Apply multiple deltas to a base graph efficiently.
///
/// Consumes the base graph. If no deltas, the base is returned as-is.
/// Otherwise, computes the final node set across ALL deltas, remaps ONCE,
/// then applies each delta's changes in order to the mutable adjacency lists.
pub fn apply_deltas(
    base: ArrayGraphSerializable,
    deltas: &[MapGraphDelta],
) -> Result<ArrayGraphSerializable> {
    if deltas.is_empty() {
        return Ok(base);
    }

    let empty_nodes = MapDelta {
        added: BTreeMap::new(),
        removed: BTreeSet::new(),
        changed: BTreeMap::new(),
    };

    // Destructure base so we can move fields instead of cloning.
    let ArrayGraphSerializable {
        node_names_ordered: base_node_names,
        edges: base_edges,
        node_metadata: base_node_metadata,
        graph_settings: base_graph_settings,
        traversal_config: base_traversal_config,
        budget_configs: base_budget_configs,
        entry_points: base_entry_points,
    } = base;

    // Phase 1: Compute the final set of node names by replaying add/remove
    // operations across all deltas. We only allocate for delta-sized sets,
    // not the full base node list.
    //
    // We track which names are added/removed relative to the base using
    // a small set that replays the net effect of all deltas.
    let mut delta_names: BTreeSet<String> = BTreeSet::new();
    let mut delta_removed: BTreeSet<String> = BTreeSet::new();

    // Replay all deltas to compute net adds/removes
    for delta in deltas {
        let nodes = delta.nodes.as_ref().unwrap_or(&empty_nodes);
        for name in &nodes.removed {
            delta_names.remove(name);
            delta_removed.insert(name.clone());
        }
        for name in nodes.added.keys() {
            delta_removed.remove(name);
            delta_names.insert(name.clone());
        }
        // Edge targets pointing to non-existing nodes need indices too
        for graph_node in nodes.added.values() {
            for name in graph_node.edge_names_iter() {
                if !delta_removed.contains(name) {
                    delta_names.insert(name.clone());
                }
            }
        }
        collect_edge_target_names_into(&nodes.changed, &mut delta_names);
    }

    // Phase 2: Build final ArrayGraphNodes by merge-sorting base names + delta_names,
    // skipping delta_removed. No per-name allocation for base nodes that are unchanged.
    let final_nodes = build_final_nodes(&base_node_names, &delta_names, &delta_removed);
    let remap = build_remap_context(&base_node_names, &final_nodes);
    let remapped_edges = base_edges.remap(&remap)?;
    let remapped_metadata = base_node_metadata.remap(&remap)?;

    // Phase 3: Convert CSR to mutable adjacency lists ONCE
    let node_count = final_nodes.combined_nodes_len();
    let mut directed_adj = csr_to_adj_lists(
        &remapped_edges.directed,
        &remapped_edges.directed_offsets,
        node_count,
    );
    let mut tagged = remapped_edges.tagged;
    let mut dynamic = remapped_edges.dynamic;
    let mut metrics = remapped_metadata.metrics;
    let mut labels = remapped_metadata.labels;
    let mut properties = remapped_metadata.properties;

    // Phase 4: Apply each delta's changes in order
    for delta in deltas {
        let nodes = delta.nodes.as_ref().unwrap_or(&empty_nodes);
        apply_node_changes(
            nodes,
            &final_nodes,
            &mut directed_adj,
            &mut tagged,
            &mut dynamic,
            &mut metrics,
            &mut labels,
            &mut properties,
            node_count,
        )?;
    }

    // Phase 5: Apply top-level settings (last non-None wins)
    let mut graph_settings = base_graph_settings;
    let mut traversal_config = base_traversal_config;
    let mut budget_configs = base_budget_configs;
    let mut entry_points = base_entry_points;

    for delta in deltas {
        if let Some(ref gs_delta) = delta.graph_settings {
            graph_settings.apply_delta(gs_delta.clone())?;
        }
        if let Some(ref tc_delta) = delta.traversal_config {
            traversal_config.apply_delta(tc_delta.clone())?;
        }
        if let Some(ref bc_delta) = delta.budget_configs {
            budget_configs.apply_delta(bc_delta.clone())?;
        }
        if let Some(ref ep_delta) = delta.entry_points {
            entry_points.apply_delta(ep_delta.clone())?;
        }
    }

    // Phase 6: Rebuild CSR + assemble result ONCE
    let (directed, directed_offsets) = adj_lists_to_csr(&directed_adj, node_count);

    Ok(ArrayGraphSerializable {
        node_names_ordered: Arc::new(final_nodes),
        edges: ArrayGraphSerializableEdges {
            directed,
            directed_offsets,
            tagged,
            dynamic,
        },
        node_metadata: ArrayGraphSerializableNodeMetadata {
            metrics,
            labels,
            properties,
        },
        graph_settings,
        traversal_config,
        budget_configs,
        entry_points,
    })
}

/// Apply a single delta's node changes to the mutable graph state.
#[allow(clippy::too_many_arguments)]
fn apply_node_changes(
    nodes: &MapDelta<String, GraphNode, GraphNodeDelta>,
    final_nodes: &ArrayGraphNodes,
    directed_adj: &mut Vec<BTreeSet<NodeIDX>>,
    tagged: &mut BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    dynamic: &mut BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    metrics: &mut BTreeMap<String, Vec<f32>>,
    labels: &mut BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    properties: &mut BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    node_count: usize,
) -> Result<()> {
    // Clear removed nodes
    for removed_name in &nodes.removed {
        if let Some(idx) = final_nodes.name_to_idx_log(removed_name) {
            clear_node(
                idx,
                directed_adj,
                tagged,
                dynamic,
                metrics,
                labels,
                properties,
            );
        }
    }

    // Apply added nodes
    for (node_name, graph_node) in &nodes.added {
        let Some(src_idx) = final_nodes.name_to_idx_log(node_name) else {
            continue;
        };
        apply_graph_node(
            src_idx,
            graph_node,
            final_nodes,
            directed_adj,
            tagged,
            dynamic,
            metrics,
            labels,
            properties,
            node_count,
        );
    }

    // Apply changed nodes
    for (node_name, node_delta) in &nodes.changed {
        let Some(src_idx) = final_nodes.name_to_idx_log(node_name) else {
            continue;
        };
        apply_graph_node_delta(
            src_idx,
            node_delta,
            final_nodes,
            directed_adj,
            tagged,
            dynamic,
            metrics,
            labels,
            properties,
            node_count,
        )?;
    }

    Ok(())
}

/// Collect edge target names from changed nodes into the names set.
fn collect_edge_target_names_into(
    changed: &BTreeMap<String, GraphNodeDelta>,
    names: &mut BTreeSet<String>,
) {
    for node_delta in changed.values() {
        if let Some(OptionDelta::Changed(ref set_delta)) = node_delta.edges_directed {
            for name in &set_delta.added {
                names.insert(name.clone());
            }
        }
        if let Some(OptionDelta::Set(ref edges)) = node_delta.edges_directed {
            for name in edges {
                names.insert(name.clone());
            }
        }
        if let Some(OptionDelta::Changed(ref map_delta)) = node_delta.edges_tagged {
            for targets in map_delta.added.values() {
                for name in targets {
                    names.insert(name.clone());
                }
            }
            for inner_delta in map_delta.changed.values() {
                for name in &inner_delta.added {
                    names.insert(name.clone());
                }
            }
        }
        if let Some(OptionDelta::Set(ref edges)) = node_delta.edges_tagged {
            for targets in edges.values() {
                for name in targets {
                    names.insert(name.clone());
                }
            }
        }
        if let Some(OptionDelta::Changed(ref map_delta)) = node_delta.edges_dynamic {
            for edge_map in map_delta.added.values() {
                for edge in edge_map.values() {
                    for branch_names in edge.branches.values() {
                        for name in branch_names {
                            names.insert(name.clone());
                        }
                    }
                }
            }
        }
        if let Some(OptionDelta::Set(ref edges)) = node_delta.edges_dynamic {
            for type_map in edges.values() {
                for edge in type_map.values() {
                    for branch_names in edge.branches.values() {
                        for name in branch_names {
                            names.insert(name.clone());
                        }
                    }
                }
            }
        }
    }
}

/// Build final ArrayGraphNodes by merge-sorting base names with additions,
/// skipping removals. Avoids allocating a String per base node name.
fn build_final_nodes(
    base: &ArrayGraphNodes,
    added: &BTreeSet<String>,
    removed: &BTreeSet<String>,
) -> ArrayGraphNodes {
    let base_count = base.combined_nodes_len();
    let estimated_count = base_count + added.len();
    let mut node_names = String::new();
    let mut offsets = Vec::with_capacity(estimated_count + 1);
    offsets.push(0);

    let mut base_iter = base.combined_node_names_iter().peekable();
    let mut added_iter = added.iter().peekable();

    loop {
        match (base_iter.peek(), added_iter.peek()) {
            (Some(&base_name), Some(add_name)) => {
                let add_name = add_name.as_str();
                match base_name.cmp(add_name) {
                    std::cmp::Ordering::Less => {
                        if !removed.contains(base_name) {
                            node_names.push_str(base_name);
                            offsets.push(node_names.len());
                        }
                        base_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        // In both base and added — keep it (added overrides removal)
                        node_names.push_str(base_name);
                        offsets.push(node_names.len());
                        base_iter.next();
                        added_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        node_names.push_str(add_name);
                        offsets.push(node_names.len());
                        added_iter.next();
                    }
                }
            }
            (Some(&base_name), None) => {
                if !removed.contains(base_name) {
                    node_names.push_str(base_name);
                    offsets.push(node_names.len());
                }
                base_iter.next();
            }
            (None, Some(add_name)) => {
                node_names.push_str(add_name);
                offsets.push(node_names.len());
                added_iter.next();
            }
            (None, None) => break,
        }
    }

    ArrayGraphNodes::from_parts(node_names, offsets)
}

/// Clear all data for a removed node.
fn clear_node(
    idx: NodeIDX,
    directed_adj: &mut Vec<BTreeSet<NodeIDX>>,
    tagged: &mut BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    dynamic: &mut BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    metrics: &mut BTreeMap<String, Vec<f32>>,
    labels: &mut BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    properties: &mut BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
) {
    directed_adj[idx].clear();
    tagged.remove(&idx);
    dynamic.remove(&idx);
    for metric_vec in metrics.values_mut() {
        metric_vec[idx] = 0.0;
    }

    // Clear labels for this node from inverted index
    for node_map in labels.values_mut() {
        node_map.remove(&idx);
    }
    labels.retain(|_, node_map| !node_map.is_empty());

    // Clear properties for this node from inverted index
    for node_map in properties.values_mut() {
        node_map.remove(&idx);
    }
    properties.retain(|_, node_map| !node_map.is_empty());

    // Clear incoming directed edges from all other nodes
    for adj in directed_adj.iter_mut() {
        adj.remove(&idx);
    }

    // Clear incoming tagged edges
    for tag_map in tagged.values_mut() {
        for target_set in tag_map.values_mut() {
            target_set.remove(&idx);
        }
        tag_map.retain(|_, targets| !targets.is_empty());
    }
    tagged.retain(|_, tag_map| !tag_map.is_empty());

    // Clear incoming dynamic edges
    for type_map in dynamic.values_mut() {
        for edge_map in type_map.values_mut() {
            for edge in edge_map.values_mut() {
                for targets in edge.branches.values_mut() {
                    targets.remove(&idx);
                }
            }
        }
    }
}

/// Apply a full GraphNode (for added nodes) — set all edges, metrics, labels, properties.
#[allow(clippy::too_many_arguments)]
fn apply_graph_node(
    src_idx: NodeIDX,
    graph_node: &GraphNode,
    final_nodes: &ArrayGraphNodes,
    directed_adj: &mut Vec<BTreeSet<NodeIDX>>,
    tagged: &mut BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    dynamic: &mut BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    metrics: &mut BTreeMap<String, Vec<f32>>,
    labels: &mut BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    properties: &mut BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    node_count: usize,
) {
    // Directed edges
    if let Some(ref edges) = graph_node.edges_directed {
        for name in edges {
            if let Some(tgt_idx) = final_nodes.name_to_idx_log(name) {
                directed_adj[src_idx].insert(tgt_idx);
            }
        }
    }

    // Tagged edges
    if let Some(ref edges) = graph_node.edges_tagged {
        let idx_map: BTreeMap<Tag, BTreeSet<NodeIDX>> = edges
            .iter()
            .map(|(tag, names)| {
                (
                    tag.clone(),
                    names
                        .iter()
                        .filter_map(|name| final_nodes.name_to_idx_log(name))
                        .collect(),
                )
            })
            .collect();
        if !idx_map.is_empty() {
            tagged.insert(src_idx, idx_map);
        }
    }

    // Dynamic edges
    if let Some(ref edges) = graph_node.edges_dynamic {
        let idx_map = dynamic_edges_name_to_idx(edges, final_nodes);
        if !idx_map.is_empty() {
            dynamic.insert(src_idx, idx_map);
        }
    }

    // Metrics
    if let Some(ref node_metrics) = graph_node.metrics {
        for (metric_name, &value) in node_metrics {
            let metric_vec = metrics
                .entry(metric_name.clone())
                .or_insert_with(|| vec![0.0; node_count]);
            if metric_vec.len() < node_count {
                metric_vec.resize(node_count, 0.0);
            }
            metric_vec[src_idx] = value;
        }
    }

    // Labels — write to inverted index
    if let Some(ref node_labels) = graph_node.labels {
        for (label_name, label_values) in node_labels {
            labels
                .entry(label_name.clone())
                .or_default()
                .insert(src_idx, label_values.clone());
        }
    }

    // Properties — write to inverted index
    if let Some(ref node_properties) = graph_node.properties {
        for (prop_name, prop_value) in node_properties {
            properties
                .entry(prop_name.clone())
                .or_default()
                .insert(src_idx, prop_value.clone());
        }
    }
}

/// Apply a GraphNodeDelta (for changed nodes) — apply edge/metric/label/property deltas.
#[allow(clippy::too_many_arguments)]
fn apply_graph_node_delta(
    src_idx: NodeIDX,
    node_delta: &GraphNodeDelta,
    final_nodes: &ArrayGraphNodes,
    directed_adj: &mut Vec<BTreeSet<NodeIDX>>,
    tagged: &mut BTreeMap<NodeIDX, BTreeMap<Tag, BTreeSet<NodeIDX>>>,
    dynamic: &mut BTreeMap<
        NodeIDX,
        BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    >,
    metrics: &mut BTreeMap<String, Vec<f32>>,
    labels: &mut BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    properties: &mut BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    node_count: usize,
) -> Result<()> {
    // Directed edges
    if let Some(ref dir_delta) = node_delta.edges_directed {
        match dir_delta {
            OptionDelta::Changed(set_delta) => {
                for removed_name in &set_delta.removed {
                    if let Some(tgt_idx) = final_nodes.name_to_idx_log(removed_name) {
                        directed_adj[src_idx].remove(&tgt_idx);
                    }
                }
                for added_name in &set_delta.added {
                    if let Some(tgt_idx) = final_nodes.name_to_idx_log(added_name) {
                        directed_adj[src_idx].insert(tgt_idx);
                    }
                }
            }
            OptionDelta::Set(edges) => {
                directed_adj[src_idx].clear();
                for name in edges {
                    if let Some(tgt_idx) = final_nodes.name_to_idx_log(name) {
                        directed_adj[src_idx].insert(tgt_idx);
                    }
                }
            }
            OptionDelta::Cleared => {
                directed_adj[src_idx].clear();
            }
            OptionDelta::Unchanged => {}
        }
    }

    // Tagged edges
    if let Some(ref tag_delta) = node_delta.edges_tagged {
        match tag_delta {
            OptionDelta::Changed(map_delta) => {
                let mut serialized = tagged
                    .get(&src_idx)
                    .map(|tag_map| tagged_edges_idx_to_name(tag_map, final_nodes))
                    .unwrap_or_default();

                // Ensure keys exist for apply_delta
                for k in map_delta.changed.keys().chain(map_delta.removed.iter()) {
                    serialized.entry(k.clone()).or_default();
                }
                for (tag, inner_set_delta) in &map_delta.changed {
                    if let Some(inner_set) = serialized.get_mut(tag) {
                        for name in &inner_set_delta.removed {
                            inner_set.insert(name.clone());
                        }
                    }
                }

                serialized.apply_delta(map_delta.clone())?;
                serialized.retain(|_, targets| !targets.is_empty());

                if serialized.is_empty() {
                    tagged.remove(&src_idx);
                } else {
                    tagged.insert(src_idx, tagged_edges_name_to_idx(&serialized, final_nodes));
                }
            }
            OptionDelta::Set(edges) => {
                let idx_map: BTreeMap<Tag, BTreeSet<NodeIDX>> = edges
                    .iter()
                    .map(|(tag, names)| {
                        (
                            tag.clone(),
                            names
                                .iter()
                                .filter_map(|name| final_nodes.name_to_idx_log(name))
                                .collect(),
                        )
                    })
                    .collect();
                if idx_map.is_empty() {
                    tagged.remove(&src_idx);
                } else {
                    tagged.insert(src_idx, idx_map);
                }
            }
            OptionDelta::Cleared => {
                tagged.remove(&src_idx);
            }
            OptionDelta::Unchanged => {}
        }
    }

    // Dynamic edges
    if let Some(ref dyn_delta) = node_delta.edges_dynamic {
        match dyn_delta {
            OptionDelta::Changed(map_delta) => {
                let mut serialized = dynamic
                    .get(&src_idx)
                    .map(|type_map| dynamic_edges_idx_to_name(type_map, final_nodes))
                    .unwrap_or_default();

                for k in map_delta.changed.keys().chain(map_delta.removed.iter()) {
                    serialized.entry(k.clone()).or_default();
                }

                serialized.apply_delta(map_delta.clone())?;

                if serialized.is_empty() {
                    dynamic.remove(&src_idx);
                } else {
                    dynamic.insert(src_idx, dynamic_edges_name_to_idx(&serialized, final_nodes));
                }
            }
            OptionDelta::Set(edges) => {
                let idx_map = dynamic_edges_name_to_idx(edges, final_nodes);
                if idx_map.is_empty() {
                    dynamic.remove(&src_idx);
                } else {
                    dynamic.insert(src_idx, idx_map);
                }
            }
            OptionDelta::Cleared => {
                dynamic.remove(&src_idx);
            }
            OptionDelta::Unchanged => {}
        }
    }

    // Metrics
    if let Some(ref metrics_delta) = node_delta.metrics {
        match metrics_delta {
            OptionDelta::Changed(map_delta) => {
                // Remove metrics
                for metric_name in &map_delta.removed {
                    if let Some(metric_vec) = metrics.get_mut(metric_name) {
                        metric_vec[src_idx] = 0.0;
                    }
                }
                // Add metrics
                for (metric_name, &value) in &map_delta.added {
                    let metric_vec = metrics
                        .entry(metric_name.clone())
                        .or_insert_with(|| vec![0.0; node_count]);
                    if metric_vec.len() < node_count {
                        metric_vec.resize(node_count, 0.0);
                    }
                    metric_vec[src_idx] = value;
                }
                // Change metrics
                for (metric_name, &value) in &map_delta.changed {
                    let metric_vec = metrics
                        .entry(metric_name.clone())
                        .or_insert_with(|| vec![0.0; node_count]);
                    if metric_vec.len() < node_count {
                        metric_vec.resize(node_count, 0.0);
                    }
                    metric_vec[src_idx] = value;
                }
            }
            OptionDelta::Set(new_metrics) => {
                // Clear all existing metrics for this node
                for metric_vec in metrics.values_mut() {
                    metric_vec[src_idx] = 0.0;
                }
                // Set new ones
                for (metric_name, &value) in new_metrics {
                    let metric_vec = metrics
                        .entry(metric_name.clone())
                        .or_insert_with(|| vec![0.0; node_count]);
                    if metric_vec.len() < node_count {
                        metric_vec.resize(node_count, 0.0);
                    }
                    metric_vec[src_idx] = value;
                }
            }
            OptionDelta::Cleared => {
                for metric_vec in metrics.values_mut() {
                    metric_vec[src_idx] = 0.0;
                }
            }
            OptionDelta::Unchanged => {}
        }
    }

    // Labels — collect per-node view from inverted index, apply delta, write back
    if let Some(ref labels_delta) = node_delta.labels {
        match labels_delta {
            OptionDelta::Changed(map_delta) => {
                let mut per_node = collect_labels_for_node(labels, src_idx);
                for k in map_delta.changed.keys().chain(map_delta.removed.iter()) {
                    per_node.entry(k.clone()).or_default();
                }
                per_node.apply_delta(map_delta.clone())?;
                per_node.retain(|_, values| !values.is_empty());
                write_labels_for_node(labels, src_idx, per_node);
            }
            OptionDelta::Set(new_labels) => {
                clear_labels_for_node(labels, src_idx);
                if !new_labels.is_empty() {
                    for (label_name, label_values) in new_labels {
                        labels
                            .entry(label_name.clone())
                            .or_default()
                            .insert(src_idx, label_values.clone());
                    }
                }
            }
            OptionDelta::Cleared => {
                clear_labels_for_node(labels, src_idx);
            }
            OptionDelta::Unchanged => {}
        }
    }

    // Properties — collect per-node view from inverted index, apply delta, write back
    if let Some(ref props_delta) = node_delta.properties {
        match props_delta {
            OptionDelta::Changed(map_delta) => {
                let mut per_node = collect_properties_for_node(properties, src_idx);
                for k in map_delta.changed.keys().chain(map_delta.removed.iter()) {
                    per_node.entry(k.clone()).or_default();
                }
                per_node.apply_delta(map_delta.clone())?;
                write_properties_for_node(properties, src_idx, per_node);
            }
            OptionDelta::Set(new_props) => {
                clear_properties_for_node(properties, src_idx);
                if !new_props.is_empty() {
                    for (prop_name, prop_value) in new_props {
                        properties
                            .entry(prop_name.clone())
                            .or_default()
                            .insert(src_idx, prop_value.clone());
                    }
                }
            }
            OptionDelta::Cleared => {
                clear_properties_for_node(properties, src_idx);
            }
            OptionDelta::Unchanged => {}
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Inverted-index helpers for labels and properties
// ---------------------------------------------------------------------------

/// Collect all labels for a specific node from the inverted labels index.
fn collect_labels_for_node(
    labels: &BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    node_idx: NodeIDX,
) -> BTreeMap<String, BTreeSet<String>> {
    labels
        .iter()
        .filter_map(|(label_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|values| (label_name.clone(), values.clone()))
        })
        .collect()
}

/// Clear all labels for a specific node from the inverted labels index.
fn clear_labels_for_node(
    labels: &mut BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    node_idx: NodeIDX,
) {
    for node_map in labels.values_mut() {
        node_map.remove(&node_idx);
    }
    labels.retain(|_, node_map| !node_map.is_empty());
}

/// Write per-node labels back to the inverted labels index.
fn write_labels_for_node(
    labels: &mut BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    node_idx: NodeIDX,
    per_node: BTreeMap<String, BTreeSet<String>>,
) {
    // First clear existing entries for this node
    clear_labels_for_node(labels, node_idx);
    // Then write new entries
    for (label_name, label_values) in per_node {
        if !label_values.is_empty() {
            labels
                .entry(label_name)
                .or_default()
                .insert(node_idx, label_values);
        }
    }
}

/// Collect all properties for a specific node from the inverted properties index.
fn collect_properties_for_node(
    properties: &BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    node_idx: NodeIDX,
) -> BTreeMap<String, String> {
    properties
        .iter()
        .filter_map(|(prop_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|value| (prop_name.clone(), value.clone()))
        })
        .collect()
}

/// Clear all properties for a specific node from the inverted properties index.
fn clear_properties_for_node(
    properties: &mut BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    node_idx: NodeIDX,
) {
    for node_map in properties.values_mut() {
        node_map.remove(&node_idx);
    }
    properties.retain(|_, node_map| !node_map.is_empty());
}

/// Write per-node properties back to the inverted properties index.
fn write_properties_for_node(
    properties: &mut BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
    node_idx: NodeIDX,
    per_node: BTreeMap<String, String>,
) {
    clear_properties_for_node(properties, node_idx);
    for (prop_name, prop_value) in per_node {
        properties
            .entry(prop_name)
            .or_default()
            .insert(node_idx, prop_value);
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build a remap context by merge-walking two sorted node lists.
/// O(N) instead of O(N log N) — avoids binary search per node.
fn build_remap_context(base_nodes: &ArrayGraphNodes, new_nodes: &ArrayGraphNodes) -> RemapContext {
    let base_count = base_nodes.combined_nodes_len();
    let new_count = new_nodes.combined_nodes_len();
    let mut mappings: Vec<Option<NodeIDX>> = vec![None; base_count];
    let mut original_positions: Vec<Option<NodeIDX>> = vec![None; new_count];

    let mut new_iter = new_nodes.combined_node_idx_iter().peekable();

    for old_idx in base_nodes.combined_node_idx_iter() {
        let base_name = base_nodes.idx_to_name(old_idx);

        // Advance new_iter until we find a name >= base_name
        while let Some(&new_idx) = new_iter.peek() {
            let new_name = new_nodes.idx_to_name(new_idx);
            match new_name.cmp(base_name) {
                std::cmp::Ordering::Less => {
                    // new node not in base — skip
                    new_iter.next();
                }
                std::cmp::Ordering::Equal => {
                    // Match — record mapping
                    mappings[old_idx] = Some(new_idx);
                    original_positions[new_idx] = Some(old_idx);
                    new_iter.next();
                    break;
                }
                std::cmp::Ordering::Greater => {
                    // base node was removed (not in new) — mappings[old_idx] stays None
                    break;
                }
            }
        }
    }

    RemapContext {
        original_positions,
        mappings,
    }
}

fn csr_to_adj_lists(
    directed: &[NodeIDX],
    offsets: &[usize],
    node_count: usize,
) -> Vec<BTreeSet<NodeIDX>> {
    let mut adj: Vec<BTreeSet<NodeIDX>> = vec![BTreeSet::new(); node_count];
    for i in 0..node_count {
        let start = offsets[i];
        let end = offsets[i + 1];
        for &target in &directed[start..end] {
            adj[i].insert(target);
        }
    }
    adj
}

fn adj_lists_to_csr(adj: &[BTreeSet<NodeIDX>], node_count: usize) -> (Vec<NodeIDX>, Vec<usize>) {
    let total_edges: usize = adj.iter().map(|s| s.len()).sum();
    let mut directed = Vec::with_capacity(total_edges);
    let mut offsets = Vec::with_capacity(node_count + 1);
    offsets.push(0);
    for adj_set in adj.iter().take(node_count) {
        for &target in adj_set {
            directed.push(target);
        }
        offsets.push(directed.len());
    }
    (directed, offsets)
}

// ---------------------------------------------------------------------------
// Name ↔ Index conversion helpers
// ---------------------------------------------------------------------------

fn tagged_edges_idx_to_name(
    tag_map: &BTreeMap<Tag, BTreeSet<NodeIDX>>,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<Tag, BTreeSet<String>> {
    tag_map
        .iter()
        .map(|(tag, targets)| {
            (
                tag.clone(),
                targets
                    .iter()
                    .map(|&idx| nodes.idx_to_name(idx).to_string())
                    .collect(),
            )
        })
        .collect()
}

fn tagged_edges_name_to_idx(
    serialized: &BTreeMap<Tag, BTreeSet<String>>,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<Tag, BTreeSet<NodeIDX>> {
    serialized
        .iter()
        .map(|(tag, names)| {
            (
                tag.clone(),
                names
                    .iter()
                    .filter_map(|name| nodes.name_to_idx_log(name))
                    .collect(),
            )
        })
        .collect()
}

fn dynamic_edges_idx_to_name(
    type_map: &BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>> {
    type_map
        .iter()
        .map(|(type_key, edge_map)| {
            let inner = edge_map
                .iter()
                .map(|(edge_name, edge)| {
                    let branches = edge
                        .branches
                        .iter()
                        .map(|(branch, idxs)| {
                            (
                                branch.clone(),
                                idxs.iter()
                                    .map(|&idx| nodes.idx_to_name(idx).to_string())
                                    .collect(),
                            )
                        })
                        .collect();
                    (
                        edge_name.clone(),
                        DynamicEdge {
                            branches,
                            metadata: edge.metadata.clone(),
                        },
                    )
                })
                .collect();
            (type_key.clone(), inner)
        })
        .collect()
}

fn dynamic_edges_name_to_idx(
    serialized: &BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdge>>,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>> {
    serialized
        .iter()
        .map(|(type_key, edge_map)| {
            let inner = edge_map
                .iter()
                .map(|(edge_name, de)| {
                    let branches = de
                        .branches
                        .iter()
                        .map(|(branch, names)| {
                            (
                                branch.clone(),
                                names
                                    .iter()
                                    .filter_map(|name| nodes.name_to_idx_log(name))
                                    .collect(),
                            )
                        })
                        .collect();
                    (
                        edge_name.clone(),
                        ArrayGraphDynamicEdge {
                            branches,
                            metadata: de.metadata.clone(),
                        },
                    )
                })
                .collect();
            (type_key.clone(), inner)
        })
        .collect()
}

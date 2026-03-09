// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
use unigraph_delta::Deltable;
use unigraph_delta::apply_option_delta;

use super::DynamicEdgeSerialized;
use super::DynamicEdgesMap;
use super::GraphDelta;
use super::TaggedEdgesMap;
use crate::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::ArrayGraphSerializableEdges;
use crate::ArrayGraphSerializableNodeMetadata;
use crate::NodeIDX;
use crate::remap_utils::RemapContext;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::Tag;

/// Apply a single delta to a base graph, producing the resulting graph.
pub fn apply_delta(
    base: &ArrayGraphSerializable,
    delta: &GraphDelta,
) -> Result<ArrayGraphSerializable> {
    apply_deltas(base, std::slice::from_ref(delta))
}

/// Apply multiple deltas to a base graph efficiently.
///
/// This batches all node additions/removals across all deltas, builds the
/// final node name list once, then applies edge/metric/tag_set changes in order.
/// This is O(N + E + sum_of_delta_sizes), not O(K * (N + E)).
pub fn apply_deltas(
    base: &ArrayGraphSerializable,
    deltas: &[GraphDelta],
) -> Result<ArrayGraphSerializable> {
    if deltas.is_empty() {
        return Ok(clone_serializable(base));
    }

    // Phase 1: Compute final node set
    let mut live_nodes: BTreeSet<String> = base
        .node_names_ordered
        .combined_node_names_iter()
        .map(|s| s.to_string())
        .collect();

    for delta in deltas {
        for name in &delta.nodes_removed {
            live_nodes.remove(name);
        }
        for name in &delta.nodes_added {
            live_nodes.insert(name.clone());
        }
    }

    // Phase 2: Build final ArrayGraphNodes + remap base
    let final_nodes = build_array_graph_nodes_from_sorted(&live_nodes);
    let remap = build_remap_context(&base.node_names_ordered, &final_nodes);
    let remapped_edges = base.edges.remap(&remap)?;
    let remapped_metadata = base.node_metadata.remap(&remap)?;

    // Phase 3: Convert CSR to mutable adjacency lists
    let node_count = final_nodes.combined_nodes_len();
    let mut directed_adj = csr_to_adj_lists(
        &remapped_edges.directed,
        &remapped_edges.directed_offsets,
        node_count,
    );
    let mut tagged = remapped_edges.tagged;
    let mut dynamic = remapped_edges.dynamic;
    let mut metrics = remapped_metadata.metrics;
    let mut tag_sets = remapped_metadata.tag_sets;

    // Phase 4: Apply all changes in order
    for delta in deltas {
        // Clear all data for nodes removed by this delta that still exist in
        // the final node set. This handles the case where a node is removed
        // by one delta and re-added by a later delta: the removal must clear
        // all edges, metrics, and tag sets before the re-add populates them.
        for removed_name in &delta.nodes_removed {
            if let Some(idx) = final_nodes.name_to_idx_log(removed_name) {
                // Clear outgoing edges and metadata
                directed_adj[idx].clear();
                tagged.remove(&idx);
                dynamic.remove(&idx);
                tag_sets.remove(&idx);
                for metric_vec in metrics.values_mut() {
                    metric_vec[idx] = 0.0;
                }

                // Clear incoming directed edges
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

                // Clear incoming dynamic edges (remove idx from branch targets)
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
        }

        // Apply edge changes
        for (node_name, edge_delta) in &delta.edge_changes {
            let Some(src_idx) = final_nodes.name_to_idx_log(node_name) else {
                // Node doesn't exist in final graph (was removed later). Skip.
                continue;
            };

            // Directed edges
            if let Some(ref dir) = edge_delta.directed {
                for removed_name in &dir.removed {
                    if let Some(tgt_idx) = final_nodes.name_to_idx_log(removed_name) {
                        directed_adj[src_idx].remove(&tgt_idx);
                    }
                }
                for added_name in &dir.added {
                    if let Some(tgt_idx) = final_nodes.name_to_idx_log(added_name) {
                        directed_adj[src_idx].insert(tgt_idx);
                    }
                    // If target doesn't exist in final graph, silently skip.
                }
            }

            // Tagged edges (recursive delta)
            if let Some(ref tag_delta) = edge_delta.tagged {
                // Convert current NodeIDX-based tagged edges to name-based form
                let mut serialized = tagged
                    .get(&src_idx)
                    .map(|tag_map| tagged_edges_idx_to_serialized(tag_map, &final_nodes))
                    .unwrap_or_default();

                // Ensure all `changed` and `removed` keys exist so apply_delta
                // can find them. The remap may have dropped empty entries for
                // tags whose targets were all removed, but the delta still
                // references those tags.
                for k in tag_delta.changed.keys().chain(tag_delta.removed.iter()) {
                    serialized.entry(k.clone()).or_default();
                }

                // Ensure inner sets contain entries referenced by `removed` in
                // nested SetDeltas. Remap may have dropped node names whose
                // targets no longer exist in the final graph.
                for (tag, inner_set_delta) in &tag_delta.changed {
                    if let Some(inner_set) = serialized.get_mut(tag) {
                        for name in &inner_set_delta.removed {
                            inner_set.insert(name.clone());
                        }
                    }
                }

                // Apply the recursive delta in the serialized (name-based) domain
                serialized.apply_delta(tag_delta.clone())?;

                // Clean up empty entries produced by removals
                serialized.retain(|_, targets| !targets.is_empty());

                // Convert back
                if serialized.is_empty() {
                    tagged.remove(&src_idx);
                } else {
                    tagged.insert(
                        src_idx,
                        tagged_edges_serialized_to_idx(&serialized, &final_nodes),
                    );
                }
            }

            // Dynamic edges (recursive delta)
            if let Some(ref dyn_delta) = edge_delta.dynamic {
                // Convert current NodeIDX-based edges to name-based serialized form
                let mut serialized = dynamic
                    .get(&src_idx)
                    .map(|type_map| dynamic_edges_idx_to_serialized(type_map, &final_nodes))
                    .unwrap_or_default();

                // Ensure all `changed` and `removed` keys exist so apply_delta
                // can find them.
                for k in dyn_delta.changed.keys().chain(dyn_delta.removed.iter()) {
                    serialized.entry(k.clone()).or_default();
                }

                // Apply the recursive delta in the serialized (name-based) domain
                serialized.apply_delta(dyn_delta.clone())?;

                if serialized.is_empty() {
                    dynamic.remove(&src_idx);
                } else {
                    // Convert back to NodeIDX-based representation
                    dynamic.insert(
                        src_idx,
                        dynamic_edges_serialized_to_idx(&serialized, &final_nodes),
                    );
                }
            }
        }

        // Apply metric changes
        for (metric_name, changes) in &delta.metric_changes {
            let metric_vec = metrics
                .entry(metric_name.clone())
                .or_insert_with(|| vec![0.0; node_count]);

            // Ensure vec is the right length (could be shorter if metric is new)
            if metric_vec.len() < node_count {
                metric_vec.resize(node_count, 0.0);
            }

            for change in changes {
                if let Some(idx) = final_nodes.name_to_idx_log(&change.node_name) {
                    metric_vec[idx] = change.value;
                }
            }
        }

        // Apply tag set changes
        for (node_name, ts_delta) in &delta.tag_set_changes {
            let Some(node_idx) = final_nodes.name_to_idx_log(node_name) else {
                continue;
            };

            let ts_map = tag_sets.entry(node_idx).or_default();

            // Ensure all `changed` and `removed` keys exist so apply_delta
            // can find them.
            for k in ts_delta.changed.keys().chain(ts_delta.removed.iter()) {
                ts_map.entry(k.clone()).or_default();
            }

            ts_map.apply_delta(ts_delta.clone())?;

            // Clean up empty entries
            ts_map.retain(|_, tags| !tags.is_empty());
            if ts_map.is_empty() {
                tag_sets.remove(&node_idx);
            }
        }
    }

    // Phase 5: Apply top-level settings (last non-Unchanged wins)
    let mut graph_settings = base.graph_settings.clone();
    let mut traversal_config = base.traversal_config.clone();
    let mut entry_points = base.entry_points.clone();

    for delta in deltas {
        if !delta.graph_settings.is_unchanged() {
            graph_settings.apply_delta(delta.graph_settings.clone())?;
        }
        if !delta.traversal_config.is_unchanged() {
            traversal_config.apply_delta(delta.traversal_config.clone())?;
        }
        apply_option_delta(&mut entry_points, &delta.entry_points);
    }

    // Phase 6: Rebuild CSR + assemble result
    let (directed, directed_offsets) = adj_lists_to_csr(&directed_adj, node_count);

    Ok(ArrayGraphSerializable {
        node_names_ordered: Arc::new(final_nodes),
        edges: ArrayGraphSerializableEdges {
            directed,
            directed_offsets,
            tagged,
            dynamic,
        },
        node_metadata: ArrayGraphSerializableNodeMetadata { metrics, tag_sets },
        graph_settings,
        traversal_config,
        entry_points,
    })
}

/// Build an `ArrayGraphNodes` from a sorted set of names.
fn build_array_graph_nodes_from_sorted(names: &BTreeSet<String>) -> ArrayGraphNodes {
    let mut node_names = String::new();
    let mut offsets = vec![0usize];

    for name in names {
        node_names.push_str(name);
        offsets.push(node_names.len());
    }

    ArrayGraphNodes::from_parts(node_names, offsets)
}

/// Build a `RemapContext` mapping old indices (base) to new indices (target).
fn build_remap_context(base_nodes: &ArrayGraphNodes, new_nodes: &ArrayGraphNodes) -> RemapContext {
    let base_count = base_nodes.combined_nodes_len();
    let new_count = new_nodes.combined_nodes_len();

    let mut mappings: Vec<Option<NodeIDX>> = Vec::with_capacity(base_count);
    let mut original_positions: Vec<Option<NodeIDX>> = vec![None; new_count];

    for old_idx in base_nodes.combined_node_idx_iter() {
        let name = base_nodes.idx_to_name(old_idx);
        if let Some(new_idx) = new_nodes.name_to_idx_log(name) {
            mappings.push(Some(new_idx));
            original_positions[new_idx] = Some(old_idx);
        } else {
            mappings.push(None); // node was removed
        }
    }

    RemapContext {
        original_positions,
        mappings,
    }
}

/// Convert CSR (directed edges + offsets) to per-node adjacency sets.
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

/// Convert per-node adjacency sets back to CSR format.
fn adj_lists_to_csr(adj: &[BTreeSet<NodeIDX>], node_count: usize) -> (Vec<NodeIDX>, Vec<usize>) {
    let mut directed = Vec::new();
    let mut offsets = Vec::with_capacity(node_count + 1);
    offsets.push(0);

    for i in 0..node_count {
        for &target in &adj[i] {
            directed.push(target);
        }
        offsets.push(directed.len());
    }

    (directed, offsets)
}

/// Clone an `ArrayGraphSerializable` (edges and metadata are not Clone-derived).
fn clone_serializable(g: &ArrayGraphSerializable) -> ArrayGraphSerializable {
    ArrayGraphSerializable {
        node_names_ordered: g.node_names_ordered.clone(),
        edges: ArrayGraphSerializableEdges {
            directed: g.edges.directed.clone(),
            directed_offsets: g.edges.directed_offsets.clone(),
            tagged: g.edges.tagged.clone(),
            dynamic: g.edges.dynamic.clone(),
        },
        node_metadata: ArrayGraphSerializableNodeMetadata {
            metrics: g.node_metadata.metrics.clone(),
            tag_sets: g.node_metadata.tag_sets.clone(),
        },
        graph_settings: g.graph_settings.clone(),
        traversal_config: g.traversal_config.clone(),
        entry_points: g.entry_points.clone(),
    }
}

/// Convert NodeIDX-based dynamic edges to name-based serialized form.
fn dynamic_edges_idx_to_serialized(
    type_map: &BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, ArrayGraphDynamicEdge>>,
    nodes: &ArrayGraphNodes,
) -> DynamicEdgesMap {
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
                            let names: BTreeSet<String> = idxs
                                .iter()
                                .map(|&idx| nodes.idx_to_name(idx).to_string())
                                .collect();
                            (branch.clone(), names)
                        })
                        .collect();
                    (
                        edge_name.clone(),
                        DynamicEdgeSerialized {
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

/// Convert name-based serialized dynamic edges back to NodeIDX-based form.
fn dynamic_edges_serialized_to_idx(
    serialized: &DynamicEdgesMap,
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
                            let idxs: BTreeSet<NodeIDX> = names
                                .iter()
                                .filter_map(|name| nodes.name_to_idx_log(name))
                                .collect();
                            (branch.clone(), idxs)
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

/// Convert NodeIDX-based tagged edges to name-based serialized form.
fn tagged_edges_idx_to_serialized(
    tag_map: &BTreeMap<Tag, BTreeSet<NodeIDX>>,
    nodes: &ArrayGraphNodes,
) -> TaggedEdgesMap {
    tag_map
        .iter()
        .map(|(tag, targets)| {
            let names: BTreeSet<String> = targets
                .iter()
                .map(|&idx| nodes.idx_to_name(idx).to_string())
                .collect();
            (tag.clone(), names)
        })
        .collect()
}

/// Convert name-based serialized tagged edges back to NodeIDX-based form.
fn tagged_edges_serialized_to_idx(
    serialized: &TaggedEdgesMap,
    nodes: &ArrayGraphNodes,
) -> BTreeMap<Tag, BTreeSet<NodeIDX>> {
    serialized
        .iter()
        .map(|(tag, names)| {
            let idxs: BTreeSet<NodeIDX> = names
                .iter()
                .filter_map(|name| nodes.name_to_idx_log(name))
                .collect();
            (tag.clone(), idxs)
        })
        .collect()
}

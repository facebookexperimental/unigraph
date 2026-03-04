// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;

use super::GraphDelta;
use crate::ArrayGraphDynamicEdge;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::ArrayGraphSerializableEdges;
use crate::ArrayGraphSerializableNodeMetadata;
use crate::NodeIDX;
use crate::remap_utils::RemapContext;

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

            // Tagged edges
            if let Some(ref tag_delta) = edge_delta.tagged {
                for (tag, tag_changes) in &tag_delta.changes {
                    let tag_entry = tagged
                        .entry(src_idx)
                        .or_default()
                        .entry(tag.clone())
                        .or_default();

                    for removed_name in &tag_changes.removed {
                        if let Some(tgt_idx) = final_nodes.name_to_idx_log(removed_name) {
                            tag_entry.remove(&tgt_idx);
                        }
                    }
                    for added_name in &tag_changes.added {
                        if let Some(tgt_idx) = final_nodes.name_to_idx_log(added_name) {
                            tag_entry.insert(tgt_idx);
                        }
                    }
                }

                // Clean up empty entries
                if let Some(tag_map) = tagged.get_mut(&src_idx) {
                    tag_map.retain(|_, targets| !targets.is_empty());
                    if tag_map.is_empty() {
                        tagged.remove(&src_idx);
                    }
                }
            }

            // Dynamic edges (full replacement)
            if let Some(ref dyn_delta) = edge_delta.dynamic {
                if dyn_delta.replacement.is_empty() {
                    dynamic.remove(&src_idx);
                } else {
                    let edges: Vec<ArrayGraphDynamicEdge> = dyn_delta
                        .replacement
                        .iter()
                        .map(|de| {
                            let branches = de
                                .branches
                                .iter()
                                .map(|(branch, names)| {
                                    let idxs: BTreeSet<NodeIDX> = names
                                        .iter()
                                        .filter_map(|name| final_nodes.name_to_idx_log(name))
                                        .collect();
                                    (branch.clone(), idxs)
                                })
                                .collect();
                            ArrayGraphDynamicEdge {
                                branches,
                                properties: de.properties.clone(),
                            }
                        })
                        .collect();
                    dynamic.insert(src_idx, edges);
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

            for (ts_name, value_delta) in &ts_delta.changes {
                let tag_set = tag_sets
                    .entry(node_idx)
                    .or_default()
                    .entry(ts_name.clone())
                    .or_default();

                for removed in &value_delta.removed {
                    tag_set.remove(removed);
                }
                for added in &value_delta.added {
                    tag_set.insert(added.clone());
                }
            }

            // Clean up empty entries
            if let Some(ts_map) = tag_sets.get_mut(&node_idx) {
                ts_map.retain(|_, tags| !tags.is_empty());
                if ts_map.is_empty() {
                    tag_sets.remove(&node_idx);
                }
            }
        }
    }

    // Phase 5: Apply top-level settings (last non-None wins)
    let mut graph_settings = base.graph_settings.clone();
    let mut traversal_config = base.traversal_config.clone();
    let mut entry_points = base.entry_points.clone();

    for delta in deltas {
        if let Some(ref gs) = delta.graph_settings {
            graph_settings = gs.clone();
        }
        if let Some(ref tc) = delta.traversal_config {
            traversal_config = tc.clone();
        }
        if let Some(ref ep) = delta.entry_points {
            entry_points = ep.clone();
        }
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

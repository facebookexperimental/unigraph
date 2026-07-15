// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use k9::assert_equal;
use k9::snapshot;
use unigraph_delta::MapDelta;
use unigraph_delta::OptionDelta;
use unigraph_delta::SetDelta;

use super::apply::apply_delta;
use super::apply::apply_deltas;
use super::derive::derive_delta;
use super::*;
use crate::ArrayGraphSerializable;
use crate::MapGraph;
use crate::types::map_graph::GraphNode;
use crate::types::map_graph::GraphNodeDelta;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_graph(json: &str) -> Result<ArrayGraphSerializable> {
    MapGraph::from_json(json)
        .context("Failed to parse graph JSON")?
        .to_array_graph_serializable()
        .context("Failed to convert to array graph")
}

/// Format a graph as a readable ASCII string for snapshot testing.
fn format_graph(g: &ArrayGraphSerializable) -> String {
    let mut out = String::new();

    // Nodes
    let names: Vec<&str> = g.node_names_ordered.node_names_iter().collect();
    out.push_str(&format!("Nodes: {}\n", names.join(", ")));

    let n = g.node_names_ordered.len();

    // Directed edges (edges without metadata entries)
    let mut has_directed = false;
    for i in 0..n {
        let node_idx = crate::NodeIDX::from(i);
        let range = g.edges.edge_range(node_idx);
        let directed_targets: Vec<&str> = range
            .filter(|&edge_idx| {
                !g.edges
                    .edge_metadata_map
                    .contains_key(&crate::types::EdgeIDX::from(edge_idx))
            })
            .map(|edge_idx| g.node_names_ordered.idx_to_name(g.edges.edges[edge_idx]))
            .collect();
        if !directed_targets.is_empty() {
            if !has_directed {
                out.push_str("Directed edges:\n");
                has_directed = true;
            }
            let from = g.node_names_ordered.idx_to_name(crate::NodeIDX::from(i));
            out.push_str(&format!("  {} -> {}\n", from, directed_targets.join(", ")));
        }
    }

    // Tagged edges
    let mut has_tagged = false;
    for i in 0..n {
        let node_idx = crate::NodeIDX::from(i);
        let tagged = g.edges.tagged_edges_for_node(node_idx);
        if !tagged.is_empty() {
            if !has_tagged {
                out.push_str("Tagged edges:\n");
                has_tagged = true;
            }
            let from = g.node_names_ordered.idx_to_name(node_idx);
            for (tag, targets) in &tagged {
                let target_names: Vec<&str> = targets
                    .iter()
                    .map(|&idx| g.node_names_ordered.idx_to_name(idx))
                    .collect();
                out.push_str(&format!(
                    "  {} -[{}]-> {}\n",
                    from,
                    tag,
                    target_names.join(", ")
                ));
            }
        }
    }

    // Dynamic edges
    let mut has_dynamic = false;
    for i in 0..n {
        let node_idx = crate::NodeIDX::from(i);
        let dynamic = g.edges.dynamic_edges_for_node(node_idx);
        if !dynamic.is_empty() {
            if !has_dynamic {
                out.push_str("Dynamic edges:\n");
                has_dynamic = true;
            }
            let from = g.node_names_ordered.idx_to_name(node_idx);
            for (type_key, edge_map) in &dynamic {
                for (edge_name, edge_view) in edge_map {
                    for (branch, targets) in &edge_view.branches {
                        let target_names: Vec<&str> = targets
                            .iter()
                            .map(|&idx| g.node_names_ordered.idx_to_name(idx))
                            .collect();
                        out.push_str(&format!(
                            "  {} -[{}]-> {} ({}:{})\n",
                            from,
                            branch,
                            target_names.join(", "),
                            type_key,
                            edge_name,
                        ));
                    }
                }
            }
        }
    }

    // Metrics
    if !g.node_metadata.metrics.is_empty() {
        out.push_str("Metrics:\n");
        for (metric_name, values) in &g.node_metadata.metrics {
            let entries: Vec<String> = values
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != 0.0)
                .map(|(i, v)| {
                    let name = g.node_names_ordered.idx_to_name(crate::NodeIDX::from(i));
                    format!("{}={}", name, format_f32(*v))
                })
                .collect();
            if !entries.is_empty() {
                out.push_str(&format!("  {}: {}\n", metric_name, entries.join(", ")));
            }
        }
    }

    // Labels
    if !g.node_metadata.labels.is_empty() {
        out.push_str("Labels:\n");
        for (label_name, node_map) in &g.node_metadata.labels {
            for (node_idx, values) in node_map {
                let name = g.node_names_ordered.idx_to_name(*node_idx);
                let value_list: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
                out.push_str(&format!(
                    "  {}: {}=[{}]\n",
                    name,
                    label_name,
                    value_list.join(", ")
                ));
            }
        }
    }

    // Properties
    if !g.node_metadata.properties.is_empty() {
        out.push_str("Properties:\n");
        for (prop_name, node_map) in &g.node_metadata.properties {
            for (node_idx, value) in node_map {
                let name = g.node_names_ordered.idx_to_name(*node_idx);
                out.push_str(&format!("  {}: {}={}\n", name, prop_name, value));
            }
        }
    }

    out.trim_end().to_string()
}

/// Format a delta as a readable ASCII string for snapshot testing.
fn format_delta(d: &MapGraphDelta) -> String {
    let mut out = String::new();

    if let Some(ref nodes) = d.nodes {
        // Added nodes
        if !nodes.added.is_empty() {
            let names: Vec<&str> = nodes.added.keys().map(|s| s.as_str()).collect();
            out.push_str(&format!("Added nodes: {}\n", names.join(", ")));
        }
        // Removed nodes
        if !nodes.removed.is_empty() {
            let names: Vec<&str> = nodes.removed.iter().map(|s| s.as_str()).collect();
            out.push_str(&format!("Removed nodes: {}\n", names.join(", ")));
        }
        // Changed nodes
        if !nodes.changed.is_empty() {
            out.push_str("Changed nodes:\n");
            for (name, node_delta) in &nodes.changed {
                let mut parts = Vec::new();
                if node_delta.edges_directed.is_some() {
                    parts.push("edges_directed");
                }
                if node_delta.edges_tagged.is_some() {
                    parts.push("edges_tagged");
                }
                if node_delta.edges_dynamic.is_some() {
                    parts.push("edges_dynamic");
                }
                if node_delta.metrics.is_some() {
                    parts.push("metrics");
                }
                if node_delta.labels.is_some() {
                    parts.push("labels");
                }
                if node_delta.properties.is_some() {
                    parts.push("properties");
                }
                out.push_str(&format!("  {}: {}\n", name, parts.join(", ")));
            }
        }
    }

    if d.graph_settings.is_some() {
        out.push_str("Settings: changed\n");
    }
    if d.traversal_config.is_some() {
        out.push_str("Traversal config: changed\n");
    }
    if d.entry_points.is_some() {
        out.push_str("Entry points: changed\n");
    }
    if d.properties.is_some() {
        out.push_str("Graph properties: changed\n");
    }

    if out.is_empty() {
        return "<empty delta>".to_string();
    }

    out.trim_end().to_string()
}

fn format_f32(v: f32) -> String {
    if v == v.floor() {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn empty_delta() -> MapGraphDelta {
    MapGraphDelta {
        nodes: None,
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
        properties: None,
    }
}

// ---------------------------------------------------------------------------
// Test graphs (inline JSON for self-contained, readable tests)
// ---------------------------------------------------------------------------

const GRAPH_ABC: &str = r#"{
    "nodes": {
        "A": { "edges_directed": ["B", "C"], "metrics": { "size": 1 } },
        "B": { "edges_directed": ["C"], "metrics": { "size": 2 } },
        "C": { "metrics": { "size": 3 } }
    }
}"#;

const GRAPH_ABCD: &str = r#"{
    "nodes": {
        "A": { "edges_directed": ["B", "C"], "metrics": { "size": 1 } },
        "B": { "edges_directed": ["C"], "metrics": { "size": 2 } },
        "C": { "edges_directed": ["D"], "metrics": { "size": 3 } },
        "D": { "metrics": { "size": 4 } }
    }
}"#;

const GRAPH_WITH_TAGS: &str = r#"{
    "nodes": {
        "A": {
            "edges_directed": ["B"], "edges_tagged": { "lazy": ["C"] },
            "metrics": { "size": 1 }
        },
        "B": { "metrics": { "size": 2 } },
        "C": {
            "labels": { "categories": ["web", "mobile"] },
            "metrics": { "size": 3 }
        }
    }
}"#;

const GRAPH_WITH_DYNAMIC: &str = r#"{
    "nodes": {
        "A": {
            "edges_directed": ["B"],
            "edges_dynamic": {
                "ios_platform": {
                    "ios_1": {
                        "branches": { "main": ["C"], "fallback": ["D"] }
                    }
                }
            },
            "metrics": { "size": 10 }
        },
        "B": { "metrics": { "size": 20 } },
        "C": { "metrics": { "size": 30 } },
        "D": { "metrics": { "size": 40 } }
    }
}"#;

// The full-featured test graph from test_graphs/test_graph_1.json
const GRAPH_FULL: &str = r#"{
    "nodes": {
        "A": { "edges_directed": ["B", "D"], "metrics": { "size": 1 } },
        "B": { "edges_tagged": { "BL": ["C"], "RD": ["J"] }, "metrics": { "size": 1 } },
        "C": { "labels": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
        "D": { "edges_directed": ["F"], "edges_tagged": { "RDFD": ["E"] }, "metrics": { "size": 1 } },
        "E": { "metrics": { "size": 1 } },
        "F": { "edges_dynamic": { "ddd": { "ddd_1": { "branches": { "b1": ["G", "H"], "b2": ["I"] } } } }, "metrics": { "size": 1 } },
        "G": { "metrics": { "size": 1 } },
        "H": { "metrics": { "size": 1 } },
        "I": { "metrics": { "size": 1 } },
        "J": { "labels": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } }
    }
}"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_identity_delta() -> Result<()> {
    let g = make_graph(GRAPH_ABC)?;
    let delta = derive_delta(&g, &g)?;
    snapshot!(format_delta(&delta), "<empty delta>");
    assert!(delta.is_empty());

    let expected = format_graph(&g);
    let result = apply_delta(g, &delta)?;
    assert_equal!(format_graph(&result), expected);
    Ok(())
}

#[test]
fn test_add_nodes_only() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "C"], "metrics": { "size": 1 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } },
            "D": { "metrics": { "size": 4 } },
            "E": { "metrics": { "size": 5 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(format_delta(&delta), "Added nodes: D, E");

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_remove_nodes_only() -> Result<()> {
    let base = make_graph(GRAPH_ABCD)?;
    let target = make_graph(GRAPH_ABC)?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Removed nodes: D
Changed nodes:
  C: edges_directed
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_add_directed_edges() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } }
        }
    }"#,
    )?;
    let target = make_graph(GRAPH_ABC)?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  A: edges_directed
  B: edges_directed
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_remove_directed_edges() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  A: edges_directed
  B: edges_directed
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_tagged_edge_changes() -> Result<()> {
    let base = make_graph(GRAPH_WITH_TAGS)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_directed": ["B"], "edges_tagged": { "async": ["B"] },
                "metrics": { "size": 1 }
            },
            "B": { "metrics": { "size": 2 } },
            "C": {
                "labels": { "categories": ["web", "mobile"] },
                "metrics": { "size": 3 }
            }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  A: edges_tagged
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_dynamic_edge_changes() -> Result<()> {
    let base = make_graph(GRAPH_WITH_DYNAMIC)?;
    // Change dynamic edges: different branches and properties
    let target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_directed": ["B"],
                "edges_dynamic": {
                    "android_platform": {
                        "android_1": {
                            "branches": { "primary": ["D"] }
                        }
                    }
                },
                "metrics": { "size": 10 }
            },
            "B": { "metrics": { "size": 20 } },
            "C": { "metrics": { "size": 30 } },
            "D": { "metrics": { "size": 40 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  A: edges_dynamic
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_metric_changes() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "C"], "metrics": { "size": 10, "weight": 5 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 2, "weight": 3 } },
            "C": { "metrics": { "size": 3 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  A: metrics
  B: metrics
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_tag_set_changes() -> Result<()> {
    let base = make_graph(GRAPH_WITH_TAGS)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_directed": ["B"], "edges_tagged": { "lazy": ["C"] },
                "metrics": { "size": 1 }
            },
            "B": { "metrics": { "size": 2 } },
            "C": {
                "labels": { "categories": ["web", "desktop"], "priority": ["high"] },
                "metrics": { "size": 3 }
            }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  C: labels
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_entry_points_change() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let mut target = make_graph(GRAPH_ABC)?;
    target.entry_points = Some(BTreeSet::from(["A".to_string()]));

    let delta = derive_delta(&base, &target)?;
    snapshot!(format_delta(&delta), "Entry points: changed");

    let result = apply_delta(base, &delta)?;
    assert_eq!(result.entry_points, Some(BTreeSet::from(["A".to_string()])));
    Ok(())
}

#[test]
fn test_combined_changes() -> Result<()> {
    let base = make_graph(GRAPH_FULL)?;
    // Modify multiple things at once
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D", "K"], "metrics": { "size": 5 } },
            "B": { "edges_tagged": { "BL": ["C"], "RDFD": ["J"] }, "metrics": { "size": 1 } },
            "C": { "labels": { "disallow_tags": ["c", "d"] }, "metrics": { "size": 1 } },
            "D": { "edges_directed": ["F"], "edges_tagged": { "RDFD": ["E"] }, "metrics": { "size": 1 } },
            "E": { "metrics": { "size": 1 } },
            "F": { "edges_dynamic": { "ddd": { "ddd_1": { "branches": { "b1": ["G", "H"], "b2": ["I"] } } } }, "metrics": { "size": 1 } },
            "G": { "metrics": { "size": 1 } },
            "H": { "metrics": { "size": 1 } },
            "I": { "metrics": { "size": 1 } },
            "J": { "labels": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "K": { "edges_directed": ["B"], "metrics": { "size": 7 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Added nodes: K
Changed nodes:
  A: edges_directed, metrics
  B: edges_tagged
  C: labels
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_round_trip_full_graph() -> Result<()> {
    // Use the full-featured test graph and a modified version
    let base = make_graph(GRAPH_FULL)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D"], "metrics": { "size": 2 } },
            "B": { "edges_tagged": { "BL": ["C"], "RDFD": ["J"] }, "metrics": { "size": 1 } },
            "C": { "labels": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
            "D": { "edges_directed": ["F", "H"], "edges_tagged": { "RDFD": ["E"] }, "metrics": { "size": 1 } },
            "E": { "metrics": { "size": 1 } },
            "F": { "edges_directed": ["G"], "edges_dynamic": { "ddd": { "ddd_1": { "branches": { "b2": ["I"] } } } }, "metrics": { "size": 1 } },
            "G": { "metrics": { "size": 1 } },
            "I": { "metrics": { "size": 1 } },
            "J": { "labels": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "T": { "edges_directed": ["A"], "metrics": { "size": 10 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_add_node_with_edges() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "C", "X"], "metrics": { "size": 1 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } },
            "X": { "edges_directed": ["B"], "metrics": { "size": 99 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Added nodes: X
Changed nodes:
  A: edges_directed
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_remove_node_cascades_edges() -> Result<()> {
    // When C is removed, edges A->C and B->C should also disappear
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Removed nodes: C
Changed nodes:
  A: edges_directed
  B: edges_directed
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_apply_deltas_sequential_equivalence() -> Result<()> {
    // apply_deltas(base, [d1,d2,d3]) == apply(apply(apply(base, d1), d2), d3)
    let g1 = make_graph(GRAPH_ABC)?;
    let g2 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "C", "D"], "metrics": { "size": 1 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } },
            "D": { "metrics": { "size": 4 } }
        }
    }"#,
    )?;
    let g3 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D"], "metrics": { "size": 10 } },
            "B": { "metrics": { "size": 2 } },
            "D": { "edges_directed": ["B"], "metrics": { "size": 4 } }
        }
    }"#,
    )?;
    let g4 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D", "E"], "metrics": { "size": 10 } },
            "B": { "metrics": { "size": 2 } },
            "D": { "edges_directed": ["B"], "metrics": { "size": 4 } },
            "E": { "edges_directed": ["A"], "metrics": { "size": 7 } }
        }
    }"#,
    )?;

    let d1 = derive_delta(&g1, &g2)?;
    let d2 = derive_delta(&g2, &g3)?;
    let d3 = derive_delta(&g3, &g4)?;

    // Sequential
    let seq_result = apply_delta(apply_delta(apply_delta(g1, &d1)?, &d2)?, &d3)?;

    // Batched (recreate g1 since sequential consumed it)
    let g1_for_batch = make_graph(GRAPH_ABC)?;
    let batch_result = apply_deltas(g1_for_batch, &[d1, d2, d3])?;

    assert_equal!(format_graph(&batch_result), format_graph(&seq_result));
    assert_equal!(format_graph(&batch_result), format_graph(&g4));
    Ok(())
}

#[test]
fn test_apply_deltas_transient_node() -> Result<()> {
    // Delta 1: add node X with edges A->X, X->B
    // Delta 2: add edge B->X
    // Delta 3: remove node X
    // Final: no X, no edges to X
    let base = make_graph(GRAPH_ABC)?;

    let d1 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::from([(
                "X".to_string(),
                GraphNode {
                    edges_directed: Some(BTreeSet::from(["B".to_string()])),
                    ..Default::default()
                },
            )]),
            removed: BTreeSet::new(),
            changed: BTreeMap::from([(
                "A".to_string(),
                GraphNodeDelta {
                    edges_directed: Some(OptionDelta::Changed(SetDelta {
                        added: BTreeSet::from(["X".to_string()]),
                        removed: BTreeSet::new(),
                    })),
                    ..Default::default()
                },
            )]),
        }),
        ..empty_delta()
    };

    let d2 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
            changed: BTreeMap::from([(
                "B".to_string(),
                GraphNodeDelta {
                    edges_directed: Some(OptionDelta::Changed(SetDelta {
                        added: BTreeSet::from(["X".to_string()]),
                        removed: BTreeSet::new(),
                    })),
                    ..Default::default()
                },
            )]),
        }),
        ..empty_delta()
    };

    let d3 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::new(),
            removed: BTreeSet::from(["X".to_string()]),
            changed: BTreeMap::new(),
        }),
        ..empty_delta()
    };

    let result = apply_deltas(base, &[d1, d2, d3])?;
    // X is gone, edges to X are gone. Base graph is back to original.
    snapshot!(
        format_graph(&result),
        "
Nodes: A, B, C
Directed edges:
  A -> B, C
  B -> C
Metrics:
  size: A=1, B=2, C=3
"
    );
    Ok(())
}

#[test]
fn test_apply_deltas_edge_add_then_remove() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "A": { "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } }
        }
    }"#,
    )?;

    let d1 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
            changed: BTreeMap::from([(
                "A".to_string(),
                GraphNodeDelta {
                    edges_directed: Some(OptionDelta::Changed(SetDelta {
                        added: BTreeSet::from(["B".to_string()]),
                        removed: BTreeSet::new(),
                    })),
                    ..Default::default()
                },
            )]),
        }),
        ..empty_delta()
    };

    let d2 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
            changed: BTreeMap::from([(
                "A".to_string(),
                GraphNodeDelta {
                    edges_directed: Some(OptionDelta::Changed(SetDelta {
                        added: BTreeSet::new(),
                        removed: BTreeSet::from(["B".to_string()]),
                    })),
                    ..Default::default()
                },
            )]),
        }),
        ..empty_delta()
    };

    let result = apply_deltas(base, &[d1, d2])?;
    snapshot!(
        format_graph(&result),
        "
Nodes: A, B
Metrics:
  size: A=1, B=2
"
    );
    Ok(())
}

#[test]
fn test_apply_deltas_metric_overwrite_ordering() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;

    let d1 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
            changed: BTreeMap::from([(
                "A".to_string(),
                GraphNodeDelta {
                    metrics: Some(OptionDelta::Changed(MapDelta {
                        added: BTreeMap::new(),
                        removed: BTreeSet::new(),
                        changed: BTreeMap::from([("size".to_string(), 5.0f32)]),
                    })),
                    ..Default::default()
                },
            )]),
        }),
        ..empty_delta()
    };

    let d2 = MapGraphDelta {
        nodes: Some(MapDelta {
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
            changed: BTreeMap::from([(
                "A".to_string(),
                GraphNodeDelta {
                    metrics: Some(OptionDelta::Changed(MapDelta {
                        added: BTreeMap::new(),
                        removed: BTreeSet::new(),
                        changed: BTreeMap::from([("size".to_string(), 10.0f32)]),
                    })),
                    ..Default::default()
                },
            )]),
        }),
        ..empty_delta()
    };

    let result = apply_deltas(base, &[d1, d2])?;
    snapshot!(
        format_graph(&result),
        "
Nodes: A, B, C
Directed edges:
  A -> B, C
  B -> C
Metrics:
  size: A=10, B=2, C=3
"
    );
    Ok(())
}

#[test]
fn test_empty_deltas() -> Result<()> {
    let expected = format_graph(&make_graph(GRAPH_ABC)?);

    // Empty slice
    let result = apply_deltas(make_graph(GRAPH_ABC)?, &[])?;
    assert_equal!(format_graph(&result), expected.clone());

    // Two empty deltas
    let result = apply_deltas(make_graph(GRAPH_ABC)?, &[empty_delta(), empty_delta()])?;
    assert_equal!(format_graph(&result), expected);
    Ok(())
}

#[test]
fn test_large_batch() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "root": { "metrics": { "size": 1 } }
        }
    }"#,
    )?;

    // Create 100 deltas, each adding one node with an edge from root
    let deltas: Vec<MapGraphDelta> = (0..100)
        .map(|i| {
            let name = format!("node_{:03}", i);
            MapGraphDelta {
                nodes: Some(MapDelta {
                    added: BTreeMap::from([(
                        name.clone(),
                        GraphNode {
                            metrics: Some(BTreeMap::from([("size".to_string(), (i + 1) as f32)])),
                            ..Default::default()
                        },
                    )]),
                    removed: BTreeSet::new(),
                    changed: BTreeMap::from([(
                        "root".to_string(),
                        GraphNodeDelta {
                            edges_directed: Some(OptionDelta::Changed(SetDelta {
                                added: BTreeSet::from([name]),
                                removed: BTreeSet::new(),
                            })),
                            ..Default::default()
                        },
                    )]),
                }),
                ..empty_delta()
            }
        })
        .collect();

    let result = apply_deltas(base, &deltas)?;

    // Verify node count: root + 100 added
    let node_count = result.node_names_ordered.len();
    assert_eq!(node_count, 101);

    // Verify root has 100 directed edges
    let root_idx = result
        .node_names_ordered
        .name_to_idx_log("root")
        .context("root node not found")?;
    let root_edge_count = result.edges.edge_range(root_idx).count();
    assert_eq!(root_edge_count, 100);

    Ok(())
}

#[test]
fn test_dynamic_edge_removal() -> Result<()> {
    let base = make_graph(GRAPH_WITH_DYNAMIC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 10 } },
            "B": { "metrics": { "size": 20 } },
            "C": { "metrics": { "size": 30 } },
            "D": { "metrics": { "size": 40 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  A: edges_dynamic
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_tag_set_removal() -> Result<()> {
    let base = make_graph(GRAPH_WITH_TAGS)?;
    // Remove all tag sets from C
    let target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_directed": ["B"], "edges_tagged": { "lazy": ["C"] },
                "metrics": { "size": 1 }
            },
            "B": { "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Changed nodes:
  C: labels
"
    );

    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_apply_deltas_with_all_edge_types() -> Result<()> {
    // Start with a graph that has directed, tagged, and dynamic edges
    let base = make_graph(GRAPH_FULL)?;

    // Delta 1: change tagged edge on B, add a node K
    let g2 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D"], "metrics": { "size": 1 } },
            "B": { "edges_tagged": { "BL": ["C", "K"], "RD": ["J"] }, "metrics": { "size": 1 } },
            "C": { "labels": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
            "D": { "edges_directed": ["F"], "edges_tagged": { "RDFD": ["E"] }, "metrics": { "size": 1 } },
            "E": { "metrics": { "size": 1 } },
            "F": { "edges_dynamic": { "ddd": { "ddd_1": { "branches": { "b1": ["G", "H"], "b2": ["I"] } } } }, "metrics": { "size": 1 } },
            "G": { "metrics": { "size": 1 } },
            "H": { "metrics": { "size": 1 } },
            "I": { "metrics": { "size": 1 } },
            "J": { "labels": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "K": { "metrics": { "size": 5 } }
        }
    }"#,
    )?;

    // Delta 2: change dynamic edge on F
    let g3 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D"], "metrics": { "size": 1 } },
            "B": { "edges_tagged": { "BL": ["C", "K"], "RD": ["J"] }, "metrics": { "size": 1 } },
            "C": { "labels": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
            "D": { "edges_directed": ["F"], "edges_tagged": { "RDFD": ["E"] }, "metrics": { "size": 1 } },
            "E": { "metrics": { "size": 1 } },
            "F": { "edges_dynamic": { "eee": { "eee_1": { "branches": { "b1": ["G"] } } } }, "metrics": { "size": 1 } },
            "G": { "metrics": { "size": 1 } },
            "H": { "metrics": { "size": 1 } },
            "I": { "metrics": { "size": 1 } },
            "J": { "labels": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "K": { "metrics": { "size": 5 } }
        }
    }"#,
    )?;

    let d1 = derive_delta(&base, &g2)?;
    let d2 = derive_delta(&g2, &g3)?;

    // Batch apply
    let batch_result = apply_deltas(make_graph(GRAPH_FULL)?, &[d1.clone(), d2.clone()])?;

    // Sequential apply
    let seq_result = apply_delta(apply_delta(make_graph(GRAPH_FULL)?, &d1)?, &d2)?;

    assert_equal!(format_graph(&batch_result), format_graph(&seq_result));
    assert_equal!(format_graph(&batch_result), format_graph(&g3));
    Ok(())
}

// ---------------------------------------------------------------------------
// Serialized delta snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn test_delta_json_snapshot() -> Result<()> {
    use crate::TraversalConfig;
    use crate::graph_settings::ArrayGraphUISettings;
    use crate::graph_settings::ColumnSettings;
    use crate::graph_settings::GraphSettings;
    use crate::graph_settings::GraphStructure;
    use crate::graph_settings::SidebarPanel;
    use crate::traversal::Decision;

    // Base graph: A->B->C, with tags, dynamic edges, metrics, tag sets
    let mut base = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_directed": ["B", "C"],
                "edges_tagged": { "lazy": ["B"] },
                "edges_dynamic": {
                    "ios_platform": {
                        "ios_1": {
                            "branches": { "main": ["C"] }
                        }
                    }
                },
                "labels": { "categories": ["web", "mobile"] },
                "metrics": { "size": 10, "weight": 5 }
            },
            "B": {
                "edges_directed": ["C"],
                "metrics": { "size": 20 }
            },
            "C": {
                "labels": { "priority": ["high"] },
                "metrics": { "size": 30 }
            }
        }
    }"#,
    )?;
    base.graph_settings = Some(GraphSettings {
        description: None,
        metrics_config: None,
        metrics_visibility: None,
        ui_settings: Some(ArrayGraphUISettings {
            selected_sidebar_panel: Some(SidebarPanel::Simulation),
            columns: Some(ColumnSettings {
                hide_metrics: Some(true),
                show_counts: Some(false),
                ..Default::default()
            }),
            graph_structure: Some(GraphStructure::Forward),
            ..Default::default()
        }),
    });
    base.traversal_config = Some(TraversalConfig {
        force_nodes: Some(BTreeMap::from([
            ("A".to_string(), Decision::include()),
            ("B".to_string(), Decision::exclude()),
        ])),
        ..Default::default()
    });
    base.entry_points = Some(BTreeSet::from(["A".to_string()]));

    // Target graph: many changes
    // - Remove node C
    // - Add node D, E
    // - Change directed edges on A: remove C, add D
    // - Change tagged edges on A: remove lazy->B, add async->D
    // - Change dynamic edges on A: different platform
    // - Change metrics: A.size 10->50, B.weight added, D.size added
    // - Change tag sets on A: remove "mobile", add "desktop"
    // - Change graph settings: sidebar panel, graph structure
    // - Change traversal config: different force_nodes
    // - Change entry points
    let mut target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_directed": ["B", "D"],
                "edges_tagged": { "async": ["D"] },
                "edges_dynamic": {
                    "android_platform": {
                        "android_1": {
                            "branches": { "primary": ["D"], "fallback": ["E"] },
                            "metadata": { "version": "2.0" }
                        }
                    }
                },
                "labels": { "categories": ["web", "desktop"] },
                "metrics": { "size": 50, "weight": 5 }
            },
            "B": {
                "edges_directed": ["D"],
                "metrics": { "size": 20, "weight": 3 }
            },
            "D": {
                "edges_directed": ["E"],
                "labels": { "tier": ["t1"] },
                "metrics": { "size": 40 }
            },
            "E": {
                "metrics": { "size": 15 }
            }
        }
    }"#,
    )?;
    target.graph_settings = Some(GraphSettings {
        description: None,
        metrics_config: None,
        metrics_visibility: None,
        ui_settings: Some(ArrayGraphUISettings {
            selected_sidebar_panel: Some(SidebarPanel::GraphInfo),
            columns: Some(ColumnSettings {
                hide_metrics: Some(true),
                show_counts: Some(true),
                show_tier_column: Some(true),
                ..Default::default()
            }),
            graph_structure: Some(GraphStructure::Dominator),
            ..Default::default()
        }),
    });
    target.traversal_config = Some(TraversalConfig {
        force_nodes: Some(BTreeMap::from([
            ("A".to_string(), Decision::include()),
            ("D".to_string(), Decision::include()),
        ])),
        force_tagged: Some(BTreeMap::from([("async".to_string(), Decision::include())])),
        ..Default::default()
    });
    target.entry_points = Some(BTreeSet::from(["A".to_string(), "D".to_string()]));

    let delta = derive_delta(&base, &target)?;

    // Verify round-trip works
    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));

    // Snapshot the pretty-printed JSON of the delta
    let json = serde_json::to_string_pretty(&delta)?;
    snapshot!(
        &json,
        r#"
{
  "nodes": {
    "added": {
      "D": {
        "labels": {
          "tier": [
            "t1"
          ]
        },
        "metrics": {
          "size": 40.0
        },
        "edges_directed": [
          "E"
        ]
      },
      "E": {
        "metrics": {
          "size": 15.0
        }
      }
    },
    "removed": [
      "C"
    ],
    "changed": {
      "A": {
        "labels": {
          "changed": {
            "categories": {
              "added": [
                "desktop"
              ],
              "removed": [
                "mobile"
              ]
            }
          }
        },
        "metrics": {
          "changed": {
            "size": 50.0
          }
        },
        "edges_directed": {
          "added": [
            "D"
          ],
          "removed": [
            "C"
          ]
        },
        "edges_tagged": {
          "added": {
            "async": [
              "D"
            ]
          },
          "removed": [
            "lazy"
          ]
        },
        "edges_dynamic": {
          "added": {
            "android_platform": {
              "android_1": {
                "branches": {
                  "fallback": [
                    "E"
                  ],
                  "primary": [
                    "D"
                  ]
                },
                "metadata": {
                  "version": "2.0"
                }
              }
            }
          },
          "removed": [
            "ios_platform"
          ]
        }
      },
      "B": {
        "metrics": {
          "added": {
            "weight": 3.0
          }
        },
        "edges_directed": {
          "added": [
            "D"
          ],
          "removed": [
            "C"
          ]
        }
      }
    }
  },
  "traversal_config": {
    "force_nodes": {
      "added": {
        "D": {
          "include": true,
          "message_id": null
        }
      },
      "removed": [
        "B"
      ]
    },
    "force_tagged": {
      "set": {
        "async": {
          "include": true,
          "message_id": null
        }
      }
    }
  },
  "graph_settings": {
    "ui_settings": {
      "selected_sidebar_panel": "GraphInfo",
      "columns": {
        "show_counts": true,
        "show_tier_column": {
          "set": true
        }
      },
      "graph_structure": "Dominator"
    }
  },
  "entry_points": {
    "added": [
      "D"
    ]
  }
}
"#
    );

    // Also verify the serde round-trip of the delta itself
    let roundtripped: GraphDelta = serde_json::from_str(&json)?;
    let roundtripped_json = serde_json::to_string_pretty(&roundtripped)?;
    assert_equal!(&json, &roundtripped_json);

    Ok(())
}

#[test]
fn test_delta_json_snapshot_cleared_fields() -> Result<()> {
    use crate::TraversalConfig;
    use crate::graph_settings::ArrayGraphUISettings;
    use crate::graph_settings::GraphSettings;
    use crate::graph_settings::SidebarPanel;
    use crate::traversal::Decision;

    // Base: has settings, traversal config, and entry points
    let mut base = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } }
        }
    }"#,
    )?;
    base.graph_settings = Some(GraphSettings {
        description: None,
        metrics_config: None,
        metrics_visibility: None,
        ui_settings: Some(ArrayGraphUISettings {
            selected_sidebar_panel: Some(SidebarPanel::Simulation),
            ..Default::default()
        }),
    });
    base.traversal_config = Some(TraversalConfig {
        force_nodes: Some(BTreeMap::from([("A".to_string(), Decision::include())])),
        ..Default::default()
    });
    base.entry_points = Some(BTreeSet::from(["A".to_string()]));

    // Target: clear settings, traversal config, and entry points
    let mut target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } }
        }
    }"#,
    )?;
    target.graph_settings = None;
    target.traversal_config = None;
    target.entry_points = None;

    let delta = derive_delta(&base, &target)?;

    // Verify round-trip
    let result = apply_delta(base, &delta)?;
    assert_eq!(result.graph_settings, None);
    assert_eq!(result.traversal_config, None);
    assert_eq!(result.entry_points, None);

    let json = serde_json::to_string_pretty(&delta)?;
    snapshot!(
        &json,
        r#"
{
  "traversal_config": {
    "cleared": true
  },
  "graph_settings": {
    "cleared": true
  },
  "entry_points": {
    "cleared": true
  }
}
"#
    );

    // Verify serde round-trip
    let roundtripped: GraphDelta = serde_json::from_str(&json)?;
    let roundtripped_json = serde_json::to_string_pretty(&roundtripped)?;
    assert_equal!(&json, &roundtripped_json);

    Ok(())
}

/// Comprehensive test for TraversalConfig delta with recursive map diffing.
///
/// Two configs that share some structure but differ in many ways:
/// - force_nodes: unchanged entries, changed entries, added entries, removed entries
/// - force_edges: nested BTreeMap<K, BTreeMap<K, V>> with recursive deltas
/// - force_edges: cleared from Some to None
/// - force_edges: leaf Deltable — full replacement for changed entries
/// - label_predicates: BTreeMap with per-key diffing
/// - tiered_traversal: leaf — full replacement
/// - messages: added, removed, changed
#[test]
fn test_traversal_config_delta_comprehensive() -> Result<()> {
    use unigraph_delta::Deltable;

    use crate::TraversalConfig;
    use crate::traversal::Decision;
    use crate::traversal::DefaultBranches;
    use crate::traversal::DynamicEdgeOverride;
    use crate::traversal::DynamicTypeConfig;
    use crate::traversal::NodeLabelPredicate;
    use crate::traversal::messages::Message;
    use crate::traversal::tiered_traversal::AscendingTier;
    use crate::traversal::tiered_traversal::AscendingTiersConfig;
    use crate::traversal::tiered_traversal::TieredTraversalConfig;

    let base = TraversalConfig {
        force_nodes: Some(BTreeMap::from([
            // unchanged
            ("AppRoot".to_string(), Decision::include()),
            // will be changed (include -> exclude)
            (
                "DebugPanel".to_string(),
                Decision {
                    include: true,
                    message_id: Some("debug_msg".to_string()),
                },
            ),
            // will be removed
            ("LegacyModule".to_string(), Decision::exclude()),
        ])),
        force_edges: Some(BTreeMap::from([
            (
                "AppRoot".to_string(),
                BTreeMap::from([
                    // unchanged
                    ("Header".to_string(), Decision::include()),
                    // will change
                    ("Sidebar".to_string(), Decision::include()),
                ]),
            ),
            (
                // will be removed entirely
                "LegacyModule".to_string(),
                BTreeMap::from([("OldDep".to_string(), Decision::exclude())]),
            ),
        ])),
        force_tagged: Some(BTreeMap::from([
            ("lazy".to_string(), Decision::exclude()),
            ("async".to_string(), Decision::include()),
        ])),
        label_predicates: Some(BTreeMap::from([
            (
                "route_homepage_contains".to_string(),
                NodeLabelPredicate {
                    label_name: "route".to_string(),
                    label_value: "homepage".to_string(),
                    contains: true,
                    decision: Decision::include(),
                },
            ),
            (
                "route_homepage_not_contains".to_string(),
                NodeLabelPredicate {
                    label_name: "route".to_string(),
                    label_value: "homepage".to_string(),
                    contains: false,
                    decision: Decision::exclude(),
                },
            ),
        ])),
        force_dynamic: Some(BTreeMap::from([(
            "ios_platform".to_string(),
            DynamicTypeConfig {
                default_branches: Some(DefaultBranches::Include(vec![
                    "main".to_string(),
                    "fallback".to_string(),
                ])),
                overrides: Some(BTreeMap::from([(
                    "special_edge".to_string(),
                    DynamicEdgeOverride {
                        branches: Some(DefaultBranches::Exclude(vec!["fallback".to_string()])),
                        decision: Some(Decision::include()),
                    },
                )])),
            },
        )])),
        tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(
            AscendingTiersConfig {
                tiers: vec![
                    AscendingTier {
                        name: "initial".to_string(),
                        tags_that_transition_to_this_tier: vec![],
                        dynamic_type_keys_that_transition_to_this_tier: vec![],
                    },
                    AscendingTier {
                        name: "lazy".to_string(),
                        tags_that_transition_to_this_tier: vec!["LL".to_string()],
                        dynamic_type_keys_that_transition_to_this_tier: vec![],
                    },
                ],
                max_tier: Some(1),
            },
        )),
        messages: Some(BTreeMap::from([
            (
                "debug_msg".to_string(),
                Message("Debug panel: %points_to%".to_string()),
            ),
            (
                "legacy_msg".to_string(),
                Message("Legacy: %points_from% -> %points_to%".to_string()),
            ),
            // unchanged
            (
                "info_msg".to_string(),
                Message("Info about %points_to%".to_string()),
            ),
        ])),
    };

    let target = TraversalConfig {
        force_nodes: Some(BTreeMap::from([
            // unchanged
            ("AppRoot".to_string(), Decision::include()),
            // changed: include -> exclude, message_id removed
            ("DebugPanel".to_string(), Decision::exclude()),
            // added
            ("NewFeature".to_string(), Decision::include()),
            // LegacyModule removed
        ])),
        force_edges: Some(BTreeMap::from([
            (
                "AppRoot".to_string(),
                BTreeMap::from([
                    // unchanged
                    ("Header".to_string(), Decision::include()),
                    // changed: include -> exclude
                    ("Sidebar".to_string(), Decision::exclude()),
                    // added
                    ("Footer".to_string(), Decision::include()),
                ]),
            ),
            // LegacyModule removed
            // NewFeature added
            (
                "NewFeature".to_string(),
                BTreeMap::from([("FeatureDep".to_string(), Decision::include())]),
            ),
        ])),
        // force_tagged cleared entirely
        force_tagged: None,
        // label_predicates: per-key delta (added platform_ios, removed route_homepage_not_contains, changed route_homepage_contains)
        label_predicates: Some(BTreeMap::from([(
            "route_homepage_contains".to_string(),
            NodeLabelPredicate {
                label_name: "platform".to_string(),
                label_value: "ios".to_string(),
                contains: true,
                decision: Decision::include(),
            },
        )])),
        // force_dynamic changed: different platform config
        force_dynamic: Some(BTreeMap::from([
            // ios_platform changed (leaf — full replacement)
            (
                "ios_platform".to_string(),
                DynamicTypeConfig {
                    default_branches: Some(DefaultBranches::Include(vec!["main".to_string()])),
                    overrides: None,
                },
            ),
            // android_platform added
            (
                "android_platform".to_string(),
                DynamicTypeConfig {
                    default_branches: Some(DefaultBranches::Exclude(vec![
                        "experimental".to_string(),
                    ])),
                    overrides: None,
                },
            ),
        ])),
        // tiered_traversal changed: different tiers + no max
        tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(
            AscendingTiersConfig {
                tiers: vec![
                    AscendingTier {
                        name: "critical".to_string(),
                        tags_that_transition_to_this_tier: vec![],
                        dynamic_type_keys_that_transition_to_this_tier: vec![],
                    },
                    AscendingTier {
                        name: "deferred".to_string(),
                        tags_that_transition_to_this_tier: vec!["DF".to_string()],
                        dynamic_type_keys_that_transition_to_this_tier: vec![],
                    },
                    AscendingTier {
                        name: "background".to_string(),
                        tags_that_transition_to_this_tier: vec!["BG".to_string()],
                        dynamic_type_keys_that_transition_to_this_tier: vec![],
                    },
                ],
                max_tier: None,
            },
        )),
        messages: Some(BTreeMap::from([
            // debug_msg changed text
            (
                "debug_msg".to_string(),
                Message("Debug v2: %points_to% (from %points_from%)".to_string()),
            ),
            // legacy_msg removed
            // info_msg unchanged
            (
                "info_msg".to_string(),
                Message("Info about %points_to%".to_string()),
            ),
            // welcome_msg added
            (
                "welcome_msg".to_string(),
                Message("Welcome to %points_to%".to_string()),
            ),
        ])),
    };

    let delta = base.derive_delta(&target).unwrap();

    // Verify roundtrip: base + delta == target
    let mut result = base.clone();
    result.apply_delta(delta.clone()).unwrap();
    assert_equal!(&result, &target);

    // Verify serde roundtrip of the delta itself
    let json = serde_json::to_string_pretty(&delta)?;
    let roundtripped: <TraversalConfig as Deltable>::Delta = serde_json::from_str(&json)?;
    let roundtripped_json = serde_json::to_string_pretty(&roundtripped)?;
    assert_equal!(&json, &roundtripped_json);

    // Snapshot the full JSON delta
    snapshot!(
        &json,
        r#"
{
  "force_nodes": {
    "added": {
      "NewFeature": {
        "include": true,
        "message_id": null
      }
    },
    "removed": [
      "LegacyModule"
    ],
    "changed": {
      "DebugPanel": {
        "include": false,
        "message_id": null
      }
    }
  },
  "force_edges": {
    "added": {
      "NewFeature": {
        "FeatureDep": {
          "include": true,
          "message_id": null
        }
      }
    },
    "removed": [
      "LegacyModule"
    ],
    "changed": {
      "AppRoot": {
        "added": {
          "Footer": {
            "include": true,
            "message_id": null
          }
        },
        "changed": {
          "Sidebar": {
            "include": false,
            "message_id": null
          }
        }
      }
    }
  },
  "force_tagged": {
    "cleared": true
  },
  "label_predicates": {
    "removed": [
      "route_homepage_not_contains"
    ],
    "changed": {
      "route_homepage_contains": {
        "label_name": "platform",
        "label_value": "ios",
        "contains": true,
        "decision": {
          "include": true,
          "message_id": null
        }
      }
    }
  },
  "force_dynamic": {
    "added": {
      "android_platform": {
        "default_branches": {
          "Exclude": [
            "experimental"
          ]
        }
      }
    },
    "changed": {
      "ios_platform": {
        "default_branches": {
          "Include": [
            "main"
          ]
        }
      }
    }
  },
  "tiered_traversal": {
    "AscendingTiers": {
      "tiers": [
        {
          "name": "critical",
          "tags_that_transition_to_this_tier": [],
          "dynamic_type_keys_that_transition_to_this_tier": []
        },
        {
          "name": "deferred",
          "tags_that_transition_to_this_tier": [
            "DF"
          ],
          "dynamic_type_keys_that_transition_to_this_tier": []
        },
        {
          "name": "background",
          "tags_that_transition_to_this_tier": [
            "BG"
          ],
          "dynamic_type_keys_that_transition_to_this_tier": []
        }
      ],
      "max_tier": null
    }
  },
  "messages": {
    "added": {
      "welcome_msg": "Welcome to %points_to%"
    },
    "removed": [
      "legacy_msg"
    ],
    "changed": {
      "debug_msg": "Debug v2: %points_to% (from %points_from%)"
    }
  }
}
"#
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Bug regression: dynamic edge branch targets dropped by remap
// ---------------------------------------------------------------------------

/// Removing a node that is a dynamic edge branch target must not crash
/// delta reconstruction. The remap drops the target from the branch set
/// (since the node left the final namespace), but the delta still says
/// "remove that name from the branch." Without a pre-pass that re-inserts
/// the removed name, `BTreeSet::apply_delta` fails with
/// "removed item not found in set".
#[test]
fn test_remove_node_that_is_dynamic_edge_branch_target() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B", "C"] }
                        }
                    }
                }
            },
            "B": {},
            "C": {}
        }
    }"#,
    )?;

    // Remove C — A's dynamic edge branch "main" loses target C.
    let target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B"] }
                        }
                    }
                }
            },
            "B": {}
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

/// Same scenario but via `apply_deltas` with multiple deltas — a node
/// removed in a later delta that is a branch target in the base.
#[test]
fn test_multi_delta_remove_dynamic_edge_branch_target() -> Result<()> {
    let g0 = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B", "C"], "fallback": ["D"] }
                        }
                    }
                }
            },
            "B": {},
            "C": {},
            "D": {}
        }
    }"#,
    )?;

    // g1: change fallback branch targets
    let g1 = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B", "C"], "fallback": ["B"] }
                        }
                    }
                }
            },
            "B": {},
            "C": {},
            "D": {}
        }
    }"#,
    )?;

    // g2: remove C entirely — "main" branch loses C target
    let g2 = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B"], "fallback": ["B"] }
                        }
                    }
                }
            },
            "B": {},
            "D": {}
        }
    }"#,
    )?;

    let d1 = derive_delta(&g0, &g1)?;
    let d2 = derive_delta(&g1, &g2)?;

    let result = apply_deltas(g0, &[d1, d2])?;
    assert_equal!(format_graph(&result), format_graph(&g2));
    Ok(())
}

/// When a changed dynamic edge within an existing type key introduces edge
/// targets to nodes not in the base, those targets must be included in the
/// final node set — otherwise they are silently dropped during name→idx
/// conversion.
#[test]
fn test_changed_dynamic_edge_new_target_not_in_base() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B"] }
                        }
                    }
                }
            },
            "B": {}
        }
    }"#,
    )?;

    // Add node C and point the existing edge's branch at it.
    let target = make_graph(
        r#"{
        "nodes": {
            "A": {
                "edges_dynamic": {
                    "platform": {
                        "edge_1": {
                            "branches": { "main": ["B", "C"] }
                        }
                    }
                }
            },
            "B": {},
            "C": {}
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    let result = apply_delta(base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use k9::assert_equal;
use k9::snapshot;

use super::apply::apply_delta;
use super::apply::apply_deltas;
use super::derive::derive_delta;
use super::*;
use crate::ArrayGraphSerializable;
use crate::MapGraph;

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
    let names: Vec<&str> = g.node_names_ordered.combined_node_names_iter().collect();
    out.push_str(&format!("Nodes: {}\n", names.join(", ")));

    let n = g.node_names_ordered.combined_nodes_len();

    // Directed edges
    let mut has_directed = false;
    for i in 0..n {
        let start = g.edges.directed_offsets[i];
        let end = g.edges.directed_offsets[i + 1];
        if start < end {
            if !has_directed {
                out.push_str("Directed edges:\n");
                has_directed = true;
            }
            let from = g.node_names_ordered.idx_to_name(crate::NodeIDX::from(i));
            let targets: Vec<&str> = g.edges.directed[start..end]
                .iter()
                .map(|&idx| g.node_names_ordered.idx_to_name(idx))
                .collect();
            out.push_str(&format!("  {} -> {}\n", from, targets.join(", ")));
        }
    }

    // Tagged edges
    if !g.edges.tagged.is_empty() {
        out.push_str("Tagged edges:\n");
        for (src_idx, tag_map) in &g.edges.tagged {
            let from = g.node_names_ordered.idx_to_name(*src_idx);
            for (tag, targets) in tag_map {
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
    if !g.edges.dynamic.is_empty() {
        out.push_str("Dynamic edges:\n");
        for (src_idx, dyn_edges) in &g.edges.dynamic {
            let from = g.node_names_ordered.idx_to_name(*src_idx);
            for edge in dyn_edges {
                let props: Vec<String> = edge
                    .properties
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                let props_str = if props.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", props.join(", "))
                };
                for (branch, targets) in &edge.branches {
                    let target_names: Vec<&str> = targets
                        .iter()
                        .map(|&idx| g.node_names_ordered.idx_to_name(idx))
                        .collect();
                    out.push_str(&format!(
                        "  {} -[{}]-> {}{}\n",
                        from,
                        branch,
                        target_names.join(", "),
                        props_str
                    ));
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

    // Tag sets
    if !g.node_metadata.tag_sets.is_empty() {
        out.push_str("Tag sets:\n");
        for (node_idx, ts_map) in &g.node_metadata.tag_sets {
            let name = g.node_names_ordered.idx_to_name(*node_idx);
            for (ts_name, tags) in ts_map {
                let tag_list: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
                out.push_str(&format!(
                    "  {}: {}=[{}]\n",
                    name,
                    ts_name,
                    tag_list.join(", ")
                ));
            }
        }
    }

    out.trim_end().to_string()
}

/// Format a delta as a readable ASCII string for snapshot testing.
fn format_delta(d: &GraphDelta) -> String {
    let mut out = String::new();

    // Nodes
    if !d.nodes_added.is_empty() {
        out.push_str(&format!("Added nodes: {}\n", d.nodes_added.join(", ")));
    }
    if !d.nodes_removed.is_empty() {
        out.push_str(&format!("Removed nodes: {}\n", d.nodes_removed.join(", ")));
    }

    // Edge changes
    if !d.edge_changes.is_empty() {
        out.push_str("Edge changes:\n");
        for (name, edge_delta) in &d.edge_changes {
            if let Some(ref dir) = edge_delta.directed {
                let mut parts = Vec::new();
                for a in &dir.added {
                    parts.push(format!("+{}", a));
                }
                for r in &dir.removed {
                    parts.push(format!("-{}", r));
                }
                out.push_str(&format!("  {} directed: {}\n", name, parts.join(", ")));
            }
            if let Some(ref tag) = edge_delta.tagged {
                for (tag_name, tag_delta) in &tag.changes {
                    let mut parts = Vec::new();
                    for a in &tag_delta.added {
                        parts.push(format!("+{}", a));
                    }
                    for r in &tag_delta.removed {
                        parts.push(format!("-{}", r));
                    }
                    out.push_str(&format!(
                        "  {} tagged [{}]: {}\n",
                        name,
                        tag_name,
                        parts.join(", ")
                    ));
                }
            }
            if let Some(ref dyn_delta) = edge_delta.dynamic {
                if dyn_delta.replacement.is_empty() {
                    out.push_str(&format!("  {} dynamic: <cleared>\n", name));
                } else {
                    for de in &dyn_delta.replacement {
                        let props: Vec<String> = de
                            .properties
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v))
                            .collect();
                        let props_str = if props.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", props.join(", "))
                        };
                        for (branch, targets) in &de.branches {
                            let tgts: Vec<&str> = targets.iter().map(|s| s.as_str()).collect();
                            out.push_str(&format!(
                                "  {} dynamic [{}]: {}{}\n",
                                name,
                                branch,
                                tgts.join(", "),
                                props_str
                            ));
                        }
                    }
                }
            }
        }
    }

    // Metric changes
    if !d.metric_changes.is_empty() {
        out.push_str("Metric changes:\n");
        for (metric_name, changes) in &d.metric_changes {
            let entries: Vec<String> = changes
                .iter()
                .map(|c| format!("{}={}", c.node_name, format_f32(c.value)))
                .collect();
            out.push_str(&format!("  {}: {}\n", metric_name, entries.join(", ")));
        }
    }

    // Tag set changes
    if !d.tag_set_changes.is_empty() {
        out.push_str("Tag set changes:\n");
        for (name, ts_delta) in &d.tag_set_changes {
            for (ts_name, vd) in &ts_delta.changes {
                let mut parts = Vec::new();
                for a in &vd.added {
                    parts.push(format!("+{}", a));
                }
                for r in &vd.removed {
                    parts.push(format!("-{}", r));
                }
                out.push_str(&format!("  {} {}: {}\n", name, ts_name, parts.join(", ")));
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

fn empty_delta() -> GraphDelta {
    GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec![],
        edge_changes: BTreeMap::new(),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    }
}

// ---------------------------------------------------------------------------
// Test graphs (inline JSON for self-contained, readable tests)
// ---------------------------------------------------------------------------

const GRAPH_ABC: &str = r#"{
    "nodes": {
        "A": { "edges": { "directed": ["B", "C"] }, "metrics": { "size": 1 } },
        "B": { "edges": { "directed": ["C"] }, "metrics": { "size": 2 } },
        "C": { "edges": {}, "metrics": { "size": 3 } }
    }
}"#;

const GRAPH_ABCD: &str = r#"{
    "nodes": {
        "A": { "edges": { "directed": ["B", "C"] }, "metrics": { "size": 1 } },
        "B": { "edges": { "directed": ["C"] }, "metrics": { "size": 2 } },
        "C": { "edges": { "directed": ["D"] }, "metrics": { "size": 3 } },
        "D": { "edges": {}, "metrics": { "size": 4 } }
    }
}"#;

const GRAPH_WITH_TAGS: &str = r#"{
    "nodes": {
        "A": {
            "edges": { "directed": ["B"], "tagged": { "lazy": ["C"] } },
            "metrics": { "size": 1 }
        },
        "B": { "edges": {}, "metrics": { "size": 2 } },
        "C": {
            "edges": {},
            "tag_sets": { "categories": ["web", "mobile"] },
            "metrics": { "size": 3 }
        }
    }
}"#;

const GRAPH_WITH_DYNAMIC: &str = r#"{
    "nodes": {
        "A": {
            "edges": {
                "directed": ["B"],
                "dynamic": [{
                    "properties": { "platform": "ios" },
                    "branches": { "main": ["C"], "fallback": ["D"] }
                }]
            },
            "metrics": { "size": 10 }
        },
        "B": { "edges": {}, "metrics": { "size": 20 } },
        "C": { "edges": {}, "metrics": { "size": 30 } },
        "D": { "edges": {}, "metrics": { "size": 40 } }
    }
}"#;

// The full-featured test graph from test_graphs/test_graph_1.json
const GRAPH_FULL: &str = r#"{
    "nodes": {
        "A": { "edges": { "directed": ["B", "D"] }, "metrics": { "size": 1 } },
        "B": { "edges": { "tagged": { "BL": ["C"], "RD": ["J"] } }, "metrics": { "size": 1 } },
        "C": { "edges": {}, "tag_sets": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
        "D": { "edges": { "directed": ["F"], "tagged": { "RDFD": ["E"] } }, "metrics": { "size": 1 } },
        "E": { "edges": {}, "metrics": { "size": 1 } },
        "F": { "edges": { "dynamic": [{ "properties": { "type": "DDD" }, "branches": { "b1": ["G", "H"], "b2": ["I"] } }] }, "metrics": { "size": 1 } },
        "G": { "edges": {}, "metrics": { "size": 1 } },
        "H": { "edges": {}, "metrics": { "size": 1 } },
        "I": { "edges": {}, "metrics": { "size": 1 } },
        "J": { "edges": {}, "tag_sets": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } }
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

    let result = apply_delta(&g, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&g));
    Ok(())
}

#[test]
fn test_add_nodes_only() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B", "C"] }, "metrics": { "size": 1 } },
            "B": { "edges": { "directed": ["C"] }, "metrics": { "size": 2 } },
            "C": { "edges": {}, "metrics": { "size": 3 } },
            "D": { "edges": {}, "metrics": { "size": 4 } },
            "E": { "edges": {}, "metrics": { "size": 5 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Added nodes: D, E
Metric changes:
  size: D=4, E=5
"
    );

    let result = apply_delta(&base, &delta)?;
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
Edge changes:
  C directed: -D
"
    );

    let result = apply_delta(&base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_add_directed_edges() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B"] }, "metrics": { "size": 1 } },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "C": { "edges": {}, "metrics": { "size": 3 } }
        }
    }"#,
    )?;
    let target = make_graph(GRAPH_ABC)?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Edge changes:
  A directed: +C
  B directed: +C
"
    );

    let result = apply_delta(&base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_remove_directed_edges() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B"] }, "metrics": { "size": 1 } },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "C": { "edges": {}, "metrics": { "size": 3 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Edge changes:
  A directed: -C
  B directed: -C
"
    );

    let result = apply_delta(&base, &delta)?;
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
                "edges": { "directed": ["B"], "tagged": { "async": ["B"] } },
                "metrics": { "size": 1 }
            },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "C": {
                "edges": {},
                "tag_sets": { "categories": ["web", "mobile"] },
                "metrics": { "size": 3 }
            }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Edge changes:
  A tagged [async]: +B
  A tagged [lazy]: -C
"
    );

    let result = apply_delta(&base, &delta)?;
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
                "edges": {
                    "directed": ["B"],
                    "dynamic": [{
                        "properties": { "platform": "android" },
                        "branches": { "primary": ["D"] }
                    }]
                },
                "metrics": { "size": 10 }
            },
            "B": { "edges": {}, "metrics": { "size": 20 } },
            "C": { "edges": {}, "metrics": { "size": 30 } },
            "D": { "edges": {}, "metrics": { "size": 40 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Edge changes:
  A dynamic [primary]: D (platform=android)
"
    );

    let result = apply_delta(&base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_metric_changes() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B", "C"] }, "metrics": { "size": 10, "weight": 5 } },
            "B": { "edges": { "directed": ["C"] }, "metrics": { "size": 2, "weight": 3 } },
            "C": { "edges": {}, "metrics": { "size": 3 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Metric changes:
  size: A=10
  weight: A=5, B=3
"
    );

    let result = apply_delta(&base, &delta)?;
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
                "edges": { "directed": ["B"], "tagged": { "lazy": ["C"] } },
                "metrics": { "size": 1 }
            },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "C": {
                "edges": {},
                "tag_sets": { "categories": ["web", "desktop"], "priority": ["high"] },
                "metrics": { "size": 3 }
            }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Tag set changes:
  C categories: +desktop, -mobile
  C priority: +high
"
    );

    let result = apply_delta(&base, &delta)?;
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

    let result = apply_delta(&base, &delta)?;
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
            "A": { "edges": { "directed": ["B", "D", "K"] }, "metrics": { "size": 5 } },
            "B": { "edges": { "tagged": { "BL": ["C"], "RDFD": ["J"] } }, "metrics": { "size": 1 } },
            "C": { "edges": {}, "tag_sets": { "disallow_tags": ["c", "d"] }, "metrics": { "size": 1 } },
            "D": { "edges": { "directed": ["F"], "tagged": { "RDFD": ["E"] } }, "metrics": { "size": 1 } },
            "E": { "edges": {}, "metrics": { "size": 1 } },
            "F": { "edges": { "dynamic": [{ "properties": { "type": "DDD" }, "branches": { "b1": ["G", "H"], "b2": ["I"] } }] }, "metrics": { "size": 1 } },
            "G": { "edges": {}, "metrics": { "size": 1 } },
            "H": { "edges": {}, "metrics": { "size": 1 } },
            "I": { "edges": {}, "metrics": { "size": 1 } },
            "J": { "edges": {}, "tag_sets": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "K": { "edges": { "directed": ["B"] }, "metrics": { "size": 7 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Added nodes: K
Edge changes:
  A directed: +K
  B tagged [RD]: -J
  B tagged [RDFD]: +J
  K directed: +B
Metric changes:
  size: A=5, K=7
Tag set changes:
  C disallow_tags: +d, -b
"
    );

    let result = apply_delta(&base, &delta)?;
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
            "A": { "edges": { "directed": ["B", "D"] }, "metrics": { "size": 2 } },
            "B": { "edges": { "tagged": { "BL": ["C"], "RDFD": ["J"] } }, "metrics": { "size": 1 } },
            "C": { "edges": {}, "tag_sets": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
            "D": { "edges": { "directed": ["F", "H"], "tagged": { "RDFD": ["E"] } }, "metrics": { "size": 1 } },
            "E": { "edges": {}, "metrics": { "size": 1 } },
            "F": { "edges": { "directed": ["G"], "dynamic": [{ "properties": { "type": "DDD" }, "branches": { "b2": ["I"] } }] }, "metrics": { "size": 1 } },
            "G": { "edges": {}, "metrics": { "size": 1 } },
            "I": { "edges": {}, "metrics": { "size": 1 } },
            "J": { "edges": {}, "tag_sets": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "T": { "edges": { "directed": ["A"] }, "metrics": { "size": 10 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    let result = apply_delta(&base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));
    Ok(())
}

#[test]
fn test_add_node_with_edges() -> Result<()> {
    let base = make_graph(GRAPH_ABC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B", "C", "X"] }, "metrics": { "size": 1 } },
            "B": { "edges": { "directed": ["C"] }, "metrics": { "size": 2 } },
            "C": { "edges": {}, "metrics": { "size": 3 } },
            "X": { "edges": { "directed": ["B"] }, "metrics": { "size": 99 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Added nodes: X
Edge changes:
  A directed: +X
  X directed: +B
Metric changes:
  size: X=99
"
    );

    let result = apply_delta(&base, &delta)?;
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
            "A": { "edges": { "directed": ["B"] }, "metrics": { "size": 1 } },
            "B": { "edges": {}, "metrics": { "size": 2 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Removed nodes: C
Edge changes:
  A directed: -C
  B directed: -C
"
    );

    let result = apply_delta(&base, &delta)?;
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
            "A": { "edges": { "directed": ["B", "C", "D"] }, "metrics": { "size": 1 } },
            "B": { "edges": { "directed": ["C"] }, "metrics": { "size": 2 } },
            "C": { "edges": {}, "metrics": { "size": 3 } },
            "D": { "edges": {}, "metrics": { "size": 4 } }
        }
    }"#,
    )?;
    let g3 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B", "D"] }, "metrics": { "size": 10 } },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "D": { "edges": { "directed": ["B"] }, "metrics": { "size": 4 } }
        }
    }"#,
    )?;
    let g4 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B", "D", "E"] }, "metrics": { "size": 10 } },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "D": { "edges": { "directed": ["B"] }, "metrics": { "size": 4 } },
            "E": { "edges": { "directed": ["A"] }, "metrics": { "size": 7 } }
        }
    }"#,
    )?;

    let d1 = derive_delta(&g1, &g2)?;
    let d2 = derive_delta(&g2, &g3)?;
    let d3 = derive_delta(&g3, &g4)?;

    // Sequential
    let seq_result = apply_delta(&apply_delta(&apply_delta(&g1, &d1)?, &d2)?, &d3)?;

    // Batched
    let batch_result = apply_deltas(&g1, &[d1, d2, d3])?;

    assert_equal!(format_graph(&batch_result), format_graph(&seq_result));
    assert_equal!(format_graph(&batch_result), format_graph(&g4));
    Ok(())
}

#[test]
fn test_apply_deltas_transient_node() -> Result<()> {
    // Delta 1: add node X with edges A->X
    // Delta 2: add edge B->X
    // Delta 3: remove node X
    // Final: no X, no edges to X
    let base = make_graph(GRAPH_ABC)?;

    let d1 = GraphDelta {
        nodes_added: vec!["X".to_string()],
        nodes_removed: vec![],
        edge_changes: BTreeMap::from([
            (
                "A".to_string(),
                NodeEdgeDelta {
                    directed: Some(DirectedEdgeDelta {
                        added: BTreeSet::from(["X".to_string()]),
                        removed: BTreeSet::new(),
                    }),
                    tagged: None,
                    dynamic: None,
                },
            ),
            (
                "X".to_string(),
                NodeEdgeDelta {
                    directed: Some(DirectedEdgeDelta {
                        added: BTreeSet::from(["B".to_string()]),
                        removed: BTreeSet::new(),
                    }),
                    tagged: None,
                    dynamic: None,
                },
            ),
        ]),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let d2 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec![],
        edge_changes: BTreeMap::from([(
            "B".to_string(),
            NodeEdgeDelta {
                directed: Some(DirectedEdgeDelta {
                    added: BTreeSet::from(["X".to_string()]),
                    removed: BTreeSet::new(),
                }),
                tagged: None,
                dynamic: None,
            },
        )]),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let d3 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec!["X".to_string()],
        edge_changes: BTreeMap::new(),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let result = apply_deltas(&base, &[d1, d2, d3])?;
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
            "A": { "edges": {}, "metrics": { "size": 1 } },
            "B": { "edges": {}, "metrics": { "size": 2 } }
        }
    }"#,
    )?;

    let d1 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec![],
        edge_changes: BTreeMap::from([(
            "A".to_string(),
            NodeEdgeDelta {
                directed: Some(DirectedEdgeDelta {
                    added: BTreeSet::from(["B".to_string()]),
                    removed: BTreeSet::new(),
                }),
                tagged: None,
                dynamic: None,
            },
        )]),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let d2 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec![],
        edge_changes: BTreeMap::from([(
            "A".to_string(),
            NodeEdgeDelta {
                directed: Some(DirectedEdgeDelta {
                    added: BTreeSet::new(),
                    removed: BTreeSet::from(["B".to_string()]),
                }),
                tagged: None,
                dynamic: None,
            },
        )]),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let result = apply_deltas(&base, &[d1, d2])?;
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

    let d1 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec![],
        edge_changes: BTreeMap::new(),
        metric_changes: BTreeMap::from([(
            "size".to_string(),
            vec![MetricNodeChange {
                node_name: "A".to_string(),
                value: 5.0,
            }],
        )]),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let d2 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec![],
        edge_changes: BTreeMap::new(),
        metric_changes: BTreeMap::from([(
            "size".to_string(),
            vec![MetricNodeChange {
                node_name: "A".to_string(),
                value: 10.0,
            }],
        )]),
        tag_set_changes: BTreeMap::new(),
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
    };

    let result = apply_deltas(&base, &[d1, d2])?;
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
    let base = make_graph(GRAPH_ABC)?;

    // Empty slice
    let result = apply_deltas(&base, &[])?;
    assert_equal!(format_graph(&result), format_graph(&base));

    // Two empty deltas
    let result = apply_deltas(&base, &[empty_delta(), empty_delta()])?;
    assert_equal!(format_graph(&result), format_graph(&base));
    Ok(())
}

#[test]
fn test_large_batch() -> Result<()> {
    let base = make_graph(
        r#"{
        "nodes": {
            "root": { "edges": {}, "metrics": { "size": 1 } }
        }
    }"#,
    )?;

    // Create 100 deltas, each adding one node with an edge from root
    let deltas: Vec<GraphDelta> = (0..100)
        .map(|i| {
            let name = format!("node_{:03}", i);
            GraphDelta {
                nodes_added: vec![name.clone()],
                nodes_removed: vec![],
                edge_changes: BTreeMap::from([(
                    "root".to_string(),
                    NodeEdgeDelta {
                        directed: Some(DirectedEdgeDelta {
                            added: BTreeSet::from([name.clone()]),
                            removed: BTreeSet::new(),
                        }),
                        tagged: None,
                        dynamic: None,
                    },
                )]),
                metric_changes: BTreeMap::from([(
                    "size".to_string(),
                    vec![MetricNodeChange {
                        node_name: name,
                        value: (i + 1) as f32,
                    }],
                )]),
                tag_set_changes: BTreeMap::new(),
                graph_settings: None,
                traversal_config: None,
                entry_points: None,
            }
        })
        .collect();

    let result = apply_deltas(&base, &deltas)?;

    // Verify node count: root + 100 added
    let node_count = result.node_names_ordered.combined_nodes_len();
    assert_eq!(node_count, 101);

    // Verify root has 100 directed edges
    let root_idx = result
        .node_names_ordered
        .name_to_idx_log("root")
        .context("root node not found")?;
    let root_edge_start = result.edges.directed_offsets[root_idx];
    let root_edge_end = result.edges.directed_offsets[root_idx + 1];
    assert_eq!(root_edge_end - root_edge_start, 100);

    Ok(())
}

#[test]
fn test_dynamic_edge_removal() -> Result<()> {
    let base = make_graph(GRAPH_WITH_DYNAMIC)?;
    let target = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B"] }, "metrics": { "size": 10 } },
            "B": { "edges": {}, "metrics": { "size": 20 } },
            "C": { "edges": {}, "metrics": { "size": 30 } },
            "D": { "edges": {}, "metrics": { "size": 40 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Edge changes:
  A dynamic: <cleared>
"
    );

    let result = apply_delta(&base, &delta)?;
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
                "edges": { "directed": ["B"], "tagged": { "lazy": ["C"] } },
                "metrics": { "size": 1 }
            },
            "B": { "edges": {}, "metrics": { "size": 2 } },
            "C": { "edges": {}, "metrics": { "size": 3 } }
        }
    }"#,
    )?;

    let delta = derive_delta(&base, &target)?;
    snapshot!(
        format_delta(&delta),
        "
Tag set changes:
  C categories: -mobile, -web
"
    );

    let result = apply_delta(&base, &delta)?;
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
            "A": { "edges": { "directed": ["B", "D"] }, "metrics": { "size": 1 } },
            "B": { "edges": { "tagged": { "BL": ["C", "K"], "RD": ["J"] } }, "metrics": { "size": 1 } },
            "C": { "edges": {}, "tag_sets": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
            "D": { "edges": { "directed": ["F"], "tagged": { "RDFD": ["E"] } }, "metrics": { "size": 1 } },
            "E": { "edges": {}, "metrics": { "size": 1 } },
            "F": { "edges": { "dynamic": [{ "properties": { "type": "DDD" }, "branches": { "b1": ["G", "H"], "b2": ["I"] } }] }, "metrics": { "size": 1 } },
            "G": { "edges": {}, "metrics": { "size": 1 } },
            "H": { "edges": {}, "metrics": { "size": 1 } },
            "I": { "edges": {}, "metrics": { "size": 1 } },
            "J": { "edges": {}, "tag_sets": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "K": { "edges": {}, "metrics": { "size": 5 } }
        }
    }"#,
    )?;

    // Delta 2: change dynamic edge on F
    let g3 = make_graph(
        r#"{
        "nodes": {
            "A": { "edges": { "directed": ["B", "D"] }, "metrics": { "size": 1 } },
            "B": { "edges": { "tagged": { "BL": ["C", "K"], "RD": ["J"] } }, "metrics": { "size": 1 } },
            "C": { "edges": {}, "tag_sets": { "disallow_tags": ["b", "c"] }, "metrics": { "size": 1 } },
            "D": { "edges": { "directed": ["F"], "tagged": { "RDFD": ["E"] } }, "metrics": { "size": 1 } },
            "E": { "edges": {}, "metrics": { "size": 1 } },
            "F": { "edges": { "dynamic": [{ "properties": { "type": "EEE" }, "branches": { "b1": ["G"] } }] }, "metrics": { "size": 1 } },
            "G": { "edges": {}, "metrics": { "size": 1 } },
            "H": { "edges": {}, "metrics": { "size": 1 } },
            "I": { "edges": {}, "metrics": { "size": 1 } },
            "J": { "edges": {}, "tag_sets": { "assert_tags": ["a", "b"] }, "metrics": { "size": 1 } },
            "K": { "edges": {}, "metrics": { "size": 5 } }
        }
    }"#,
    )?;

    let d1 = derive_delta(&base, &g2)?;
    let d2 = derive_delta(&g2, &g3)?;

    // Batch apply
    let batch_result = apply_deltas(&base, &[d1.clone(), d2.clone()])?;

    // Sequential apply
    let seq_result = apply_delta(&apply_delta(&base, &d1)?, &d2)?;

    assert_equal!(format_graph(&batch_result), format_graph(&seq_result));
    assert_equal!(format_graph(&batch_result), format_graph(&g3));
    Ok(())
}

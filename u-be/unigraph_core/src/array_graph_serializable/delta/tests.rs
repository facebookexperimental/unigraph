// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use k9::assert_equal;
use k9::snapshot;
use unigraph_delta::OptionDelta;

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
        for (src_idx, type_map) in &g.edges.dynamic {
            let from = g.node_names_ordered.idx_to_name(*src_idx);
            for (type_key, edge_map) in type_map {
                for (edge_name, edge) in edge_map {
                    for (branch, targets) in &edge.branches {
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
                for (tag_name, targets) in &tag.added {
                    let parts: Vec<String> = targets.iter().map(|a| format!("+{}", a)).collect();
                    out.push_str(&format!(
                        "  {} tagged [{}]: {}\n",
                        name,
                        tag_name,
                        parts.join(", ")
                    ));
                }
                for tag_name in &tag.removed {
                    out.push_str(&format!("  {} tagged [{}]: <removed>\n", name, tag_name));
                }
                for (tag_name, tag_delta) in &tag.changed {
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
                if dyn_delta.added.is_empty()
                    && dyn_delta.removed.is_empty()
                    && dyn_delta.changed.is_empty()
                {
                    out.push_str(&format!("  {} dynamic: <cleared>\n", name));
                } else {
                    for (type_key, edge_map) in &dyn_delta.added {
                        for (edge_name, de) in edge_map {
                            for (branch, targets) in &de.branches {
                                let tgts: Vec<&str> = targets.iter().map(|s| s.as_str()).collect();
                                out.push_str(&format!(
                                    "  {} dynamic +[{}]: {} ({}:{})\n",
                                    name,
                                    branch,
                                    tgts.join(", "),
                                    type_key,
                                    edge_name,
                                ));
                            }
                        }
                    }
                    if !dyn_delta.removed.is_empty() {
                        let removed: Vec<&str> =
                            dyn_delta.removed.iter().map(|s| s.as_str()).collect();
                        out.push_str(&format!(
                            "  {} dynamic removed types: {}\n",
                            name,
                            removed.join(", ")
                        ));
                    }
                    // changed entries contain recursive deltas — show summary
                    for (type_key, _inner_delta) in &dyn_delta.changed {
                        out.push_str(&format!("  {} dynamic ~[{}]: <changed>\n", name, type_key,));
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
            for (ts_name, tags) in &ts_delta.added {
                let parts: Vec<String> = tags.iter().map(|t| format!("+{}", t)).collect();
                out.push_str(&format!("  {} {}: {}\n", name, ts_name, parts.join(", ")));
            }
            for ts_name in &ts_delta.removed {
                out.push_str(&format!("  {} {}: <removed>\n", name, ts_name));
            }
            for (ts_name, vd) in &ts_delta.changed {
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

    if !d.graph_settings.is_unchanged() {
        out.push_str("Settings: changed\n");
    }
    if !d.traversal_config.is_unchanged() {
        out.push_str("Traversal config: changed\n");
    }
    if !d.entry_points.is_unchanged() {
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
            "A": { "edges_directed": ["B", "C"], "metrics": { "size": 1 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 2 } },
            "C": { "metrics": { "size": 3 } },
            "D": { "metrics": { "size": 4 } },
            "E": { "metrics": { "size": 5 } }
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
Edge changes:
  A tagged [async]: +B
  A tagged [lazy]: <removed>
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
Edge changes:
  A dynamic +[primary]: D (android_platform:android_1)
  A dynamic removed types: ios_platform
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
Tag set changes:
  C priority: +high
  C categories: +desktop, -mobile
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
Edge changes:
  A directed: +K
  B tagged [RDFD]: +J
  B tagged [RD]: <removed>
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
    };

    let d3 = GraphDelta {
        nodes_added: vec![],
        nodes_removed: vec!["X".to_string()],
        edge_changes: BTreeMap::new(),
        metric_changes: BTreeMap::new(),
        tag_set_changes: BTreeMap::new(),
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
            "A": { "metrics": { "size": 1 } },
            "B": { "metrics": { "size": 2 } }
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
        graph_settings: OptionDelta::Unchanged,
        traversal_config: OptionDelta::Unchanged,
        entry_points: OptionDelta::Unchanged,
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
            "root": { "metrics": { "size": 1 } }
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
                graph_settings: OptionDelta::Unchanged,
                traversal_config: OptionDelta::Unchanged,
                entry_points: OptionDelta::Unchanged,
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
Edge changes:
  A dynamic removed types: ios_platform
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
Tag set changes:
  C categories: <removed>
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
    let batch_result = apply_deltas(&base, &[d1.clone(), d2.clone()])?;

    // Sequential apply
    let seq_result = apply_delta(&apply_delta(&base, &d1)?, &d2)?;

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
    let result = apply_delta(&base, &delta)?;
    assert_equal!(format_graph(&result), format_graph(&target));

    // Snapshot the pretty-printed JSON of the delta
    let json = serde_json::to_string_pretty(&delta)?;
    snapshot!(
        &json,
        r#"
{
  "nodes_added": [
    "D",
    "E"
  ],
  "nodes_removed": [
    "C"
  ],
  "edge_changes": {
    "A": {
      "directed": {
        "added": [
          "D"
        ],
        "removed": [
          "C"
        ]
      },
      "tagged": {
        "added": {
          "async": [
            "D"
          ]
        },
        "removed": [
          "lazy"
        ]
      },
      "dynamic": {
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
      "directed": {
        "added": [
          "D"
        ],
        "removed": [
          "C"
        ]
      },
      "tagged": null,
      "dynamic": null
    },
    "D": {
      "directed": {
        "added": [
          "E"
        ]
      },
      "tagged": null,
      "dynamic": null
    }
  },
  "metric_changes": {
    "size": [
      {
        "node_name": "A",
        "value": 50.0
      },
      {
        "node_name": "D",
        "value": 40.0
      },
      {
        "node_name": "E",
        "value": 15.0
      }
    ],
    "weight": [
      {
        "node_name": "B",
        "value": 3.0
      }
    ]
  },
  "tag_set_changes": {
    "A": {
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
    "D": {
      "added": {
        "tier": [
          "t1"
        ]
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
  "entry_points": {
    "set": [
      "A",
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
    let result = apply_delta(&base, &delta)?;
    assert_eq!(result.graph_settings, None);
    assert_eq!(result.traversal_config, None);
    assert_eq!(result.entry_points, None);

    let json = serde_json::to_string_pretty(&delta)?;
    snapshot!(
        &json,
        r#"
{
  "nodes_added": [],
  "nodes_removed": [],
  "edge_changes": {},
  "metric_changes": {},
  "tag_set_changes": {},
  "graph_settings": {
    "cleared": true
  },
  "traversal_config": {
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
/// - force_tagged: cleared from Some to None
/// - force_dynamic: leaf Deltable — full replacement for changed entries
/// - label_predicates: Vec is leaf — full replacement
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
        label_predicates: Some(vec![
            NodeLabelPredicate {
                tag_set_name: "route".to_string(),
                tag_name: "homepage".to_string(),
                contains: true,
                decision: Decision::include(),
            },
            NodeLabelPredicate {
                tag_set_name: "route".to_string(),
                tag_name: "homepage".to_string(),
                contains: false,
                decision: Decision::exclude(),
            },
        ]),
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
                    },
                    AscendingTier {
                        name: "lazy".to_string(),
                        tags_that_transition_to_this_tier: vec!["LL".to_string()],
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
        // label_predicates changed (Vec is leaf, so full replacement)
        label_predicates: Some(vec![NodeLabelPredicate {
            tag_set_name: "platform".to_string(),
            tag_name: "ios".to_string(),
            contains: true,
            decision: Decision::include(),
        }]),
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
                    },
                    AscendingTier {
                        name: "deferred".to_string(),
                        tags_that_transition_to_this_tier: vec!["DF".to_string()],
                    },
                    AscendingTier {
                        name: "background".to_string(),
                        tags_that_transition_to_this_tier: vec!["BG".to_string()],
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
  "label_predicates": [
    {
      "tag_set_name": "platform",
      "tag_name": "ios",
      "contains": true,
      "decision": {
        "include": true,
        "message_id": null
      }
    }
  ],
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
          "tags_that_transition_to_this_tier": []
        },
        {
          "name": "deferred",
          "tags_that_transition_to_this_tier": [
            "DF"
          ]
        },
        {
          "name": "background",
          "tags_that_transition_to_this_tier": [
            "BG"
          ]
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

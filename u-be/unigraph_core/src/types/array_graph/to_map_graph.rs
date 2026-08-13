// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::EdgeMeta;
use crate::MapGraph;
use crate::NodeIDX;
use crate::types::EdgeIDX;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;

pub fn to_map_graph(graph: &ArrayGraph) -> Result<MapGraph> {
    let mut result = MapGraph {
        nodes: Default::default(),
        traversal_config: graph.runtime.state.traversal_config.clone(),
        graph_settings: graph.graph_settings().cloned(),
        entry_points: graph.data.entry_points.clone(),
        properties: graph.data.properties.clone(),
    };

    for node_idx in graph.node_idx_iter() {
        let map_node = get_map_node(graph, node_idx);
        result
            .nodes
            .insert(graph.idx_to_name(node_idx).to_string(), map_node);
    }

    Ok(result)
}

/// Like [`to_map_graph`], but reflects the applied traversal config: only
/// reachable nodes are emitted, and each node's excluded edges (and edges to
/// unreachable nodes) are dropped. This is the "what you see in the explorer"
/// view — the basis for exporting a trimmed graph.
pub fn to_configured_map_graph(graph: &ArrayGraph) -> Result<MapGraph> {
    let mut result = MapGraph {
        nodes: Default::default(),
        // The config is already baked into the trimmed nodes/edges, so we don't
        // re-embed it — the export is a clean, self-contained view.
        traversal_config: None,
        graph_settings: graph.graph_settings().cloned(),
        entry_points: graph.data.entry_points.clone(),
        properties: graph.data.properties.clone(),
    };

    for node_idx in graph.node_idx_iter_reachable() {
        let map_node = get_configured_map_node(graph, node_idx);
        result
            .nodes
            .insert(graph.idx_to_name(node_idx).to_string(), map_node);
    }

    Ok(result)
}

pub fn get_map_node(graph: &ArrayGraph, node_idx: NodeIDX) -> GraphNode {
    let directed = collect_directed_edges(graph, node_idx);
    let tagged = collect_tagged_edges(graph, node_idx);
    let dynamic = collect_dynamic_edges(graph, node_idx);
    let labels = collect_labels(graph, node_idx);
    let properties = collect_properties(graph, node_idx);
    let metrics = collect_metrics(graph, node_idx);

    GraphNode {
        properties: none_if_empty(properties),
        labels: none_if_empty(labels),
        metrics: none_if_empty(metrics),
        edges_directed: none_if_empty(directed),
        edges_tagged: tagged,
        edges_dynamic: dynamic,
    }
}

/// Builds a [`GraphNode`] containing only edges that survive the current
/// traversal config: excluded edges are skipped, and edges pointing at
/// unreachable nodes are dropped. Node metadata (metrics/labels/properties) is
/// copied as-is since the node itself is reachable.
fn get_configured_map_node(graph: &ArrayGraph, node_idx: NodeIDX) -> GraphNode {
    let mut directed: BTreeSet<String> = BTreeSet::new();
    let mut tagged: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut dynamic: BTreeMap<String, BTreeMap<String, DynamicEdge>> = BTreeMap::new();

    for edge_idx in graph.data.edges.edge_range(node_idx) {
        if graph.runtime.edge_flags[edge_idx].is_excluded() {
            continue;
        }
        let target = graph.data.edges.edges[edge_idx];
        if graph.is_node_unreachable(target) {
            continue;
        }
        let target_name = graph.idx_to_name(target).to_string();
        match graph.data.edges.edge_meta(EdgeIDX::from(edge_idx)) {
            None => {
                directed.insert(target_name);
            }
            Some(EdgeMeta::Tagged { tag }) => {
                tagged
                    .entry(tag.to_string())
                    .or_default()
                    .insert(target_name);
            }
            Some(EdgeMeta::Dynamic {
                type_key,
                edge_name,
                branch,
                metadata,
            }) => {
                dynamic
                    .entry(type_key.to_string())
                    .or_default()
                    .entry(edge_name.to_string())
                    .or_insert_with(|| DynamicEdge {
                        branches: BTreeMap::new(),
                        metadata: metadata.clone(),
                    })
                    .branches
                    .entry(branch.to_string())
                    .or_default()
                    .insert(target_name);
            }
        }
    }

    GraphNode {
        properties: none_if_empty(collect_properties(graph, node_idx)),
        labels: none_if_empty(collect_labels(graph, node_idx)),
        metrics: none_if_empty(collect_metrics(graph, node_idx)),
        edges_directed: none_if_empty(directed),
        edges_tagged: none_if_empty(tagged),
        edges_dynamic: none_if_empty(dynamic),
    }
}

fn none_if_empty<T: IsEmpty>(v: T) -> Option<T> {
    if v.is_empty() { None } else { Some(v) }
}

trait IsEmpty {
    fn is_empty(&self) -> bool;
}

impl<K: Ord, V> IsEmpty for BTreeMap<K, V> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<V: Ord> IsEmpty for BTreeSet<V> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

fn collect_directed_edges(graph: &ArrayGraph, node_idx: NodeIDX) -> BTreeSet<String> {
    graph
        .forward_edges(node_idx)
        .filter(|(_, flags)| {
            !flags.intersects(
                crate::types::array_graph::offset_graph::edge_flags::EdgeFlags::IS_TAGGED
                    | crate::types::array_graph::offset_graph::edge_flags::EdgeFlags::IS_DYNAMIC,
            )
        })
        .map(|(target, _)| graph.idx_to_name(target).to_string())
        .collect()
}

fn collect_tagged_edges(
    graph: &ArrayGraph,
    node_idx: NodeIDX,
) -> Option<BTreeMap<String, BTreeSet<String>>> {
    let tagged_map = graph.data.edges.tagged_edges_for_node(node_idx);
    if tagged_map.is_empty() {
        return None;
    }
    Some(
        tagged_map
            .into_iter()
            .map(|(tag, targets)| {
                (
                    tag.to_string(),
                    targets
                        .into_iter()
                        .map(|t| graph.idx_to_name(t).to_string())
                        .collect(),
                )
            })
            .collect(),
    )
}

fn collect_dynamic_edges(
    graph: &ArrayGraph,
    node_idx: NodeIDX,
) -> Option<BTreeMap<String, BTreeMap<String, DynamicEdge>>> {
    let dynamic_map = graph.data.edges.dynamic_edges_for_node(node_idx);
    if dynamic_map.is_empty() {
        return None;
    }
    Some(
        dynamic_map
            .into_iter()
            .map(|(type_key, edge_map)| {
                let inner = edge_map
                    .into_iter()
                    .map(|(edge_name, edge_view)| {
                        (
                            edge_name.to_string(),
                            DynamicEdge {
                                branches: edge_view
                                    .branches
                                    .into_iter()
                                    .map(|(branch, pts)| {
                                        (
                                            branch.to_string(),
                                            pts.iter()
                                                .map(|pt| graph.idx_to_name(*pt).to_string())
                                                .collect(),
                                        )
                                    })
                                    .collect(),
                                metadata: edge_view.metadata.cloned(),
                            },
                        )
                    })
                    .collect();
                (type_key.to_string(), inner)
            })
            .collect(),
    )
}

fn collect_labels(graph: &ArrayGraph, node_idx: NodeIDX) -> BTreeMap<String, BTreeSet<String>> {
    graph
        .data
        .node_metadata
        .labels
        .iter()
        .filter_map(|(label_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|values| (label_name.clone(), values.clone()))
        })
        .collect()
}

fn collect_properties(graph: &ArrayGraph, node_idx: NodeIDX) -> BTreeMap<String, String> {
    graph
        .data
        .node_metadata
        .properties
        .iter()
        .filter_map(|(prop_name, node_map)| {
            node_map
                .get(&node_idx)
                .map(|value| (prop_name.clone(), value.clone()))
        })
        .collect()
}

fn collect_metrics(graph: &ArrayGraph, node_idx: NodeIDX) -> BTreeMap<String, f64> {
    graph
        .data
        .node_metadata
        .metrics
        .iter()
        .filter_map(|(name, values)| {
            let v = values[node_idx];
            if v != 0.0 {
                Some((name.to_string(), v))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use k9::MultilineString;
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;
    use crate::GraphBuilder;
    use crate::TraversalConfig;
    use crate::tests::test_graphs::make_test_array_graph_1;
    use crate::tests::test_graphs::make_test_graph_1;
    use crate::traversal::Decision;

    #[test]
    fn test_to_map_graph() -> Result<()> {
        let original = make_test_graph_1();
        let original_json = serde_json::to_string_pretty(&original)?;
        let g = make_test_array_graph_1()?;
        let roundtrip = g.to_map_graph()?;

        let roundtrip_json = serde_json::to_string_pretty(&roundtrip)?;

        assert_equal!(
            MultilineString(original_json.clone()),
            MultilineString(roundtrip_json.clone())
        );

        snapshot!(
            roundtrip_json,
            r#"
{
  "nodes": {
    "A": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "B",
        "D"
      ]
    },
    "B": {
      "metrics": {
        "size": 1.0
      },
      "edges_tagged": {
        "BL": [
          "C"
        ],
        "RD": [
          "J"
        ]
      }
    },
    "C": {
      "labels": {
        "disallow_tags": [
          "b",
          "c"
        ]
      },
      "metrics": {
        "size": 1.0
      }
    },
    "D": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "F"
      ],
      "edges_tagged": {
        "RDFD": [
          "E"
        ]
      }
    },
    "E": {
      "metrics": {
        "size": 1.0
      }
    },
    "F": {
      "metrics": {
        "size": 1.0
      },
      "edges_dynamic": {
        "ddd": {
          "ddd_1": {
            "branches": {
              "b1": [
                "G",
                "H"
              ],
              "b2": [
                "I"
              ]
            }
          }
        }
      }
    },
    "G": {
      "metrics": {
        "size": 1.0
      }
    },
    "H": {
      "metrics": {
        "size": 1.0
      }
    },
    "I": {
      "metrics": {
        "size": 1.0
      }
    },
    "J": {
      "labels": {
        "assert_tags": [
          "a",
          "b"
        ]
      },
      "metrics": {
        "size": 1.0
      }
    }
  },
  "traversal_config": null,
  "graph_settings": null,
  "entry_points": null
}
"#
        );
        Ok(())
    }

    /// The configured view must drop edges that were *not followed* (excluded by
    /// the traversal config), even when the edge's target is still reachable via
    /// another path. Uses a diamond (A->B, A->C, B->C) so that excluding B->C
    /// leaves C reachable from A — isolating the excluded-edge filter from the
    /// unreachable-node filter.
    #[test]
    fn test_to_configured_map_graph_drops_unfollowed_edge() -> Result<()> {
        let task = ll::Task::create_new("test");
        let mut b = GraphBuilder::new();
        b.add_edge("A", "B")?;
        b.add_edge("A", "C")?;
        b.add_edge("B", "C")?;
        let mut graph = b.build().to_array_graph(&task)?;

        // Unconfigured view keeps B->C.
        let unconfigured = graph.to_map_graph()?;
        assert_equal!(
            unconfigured.nodes["B"].edges_directed,
            Some(BTreeSet::from(["C".to_string()]))
        );

        // Exclude only the B->C edge; C stays reachable through A->C.
        let mut b_edges = BTreeMap::new();
        b_edges.insert("C".to_string(), Decision::exclude());
        let mut force_edges = BTreeMap::new();
        force_edges.insert("B".to_string(), b_edges);
        let config = TraversalConfig {
            force_nodes: None,
            force_edges: Some(force_edges),
            force_tagged: None,
            label_predicates: None,
            force_dynamic: None,
            tiered_traversal: None,
            messages: None,
        };
        graph.apply_traversal_config_and_entry_points(config)?;

        let configured = graph.to_configured_map_graph()?;

        // C survives (reachable via A->C), so it's not the unreachable filter.
        assert!(configured.nodes.contains_key("C"));
        // But B's only edge (B->C) was not followed, so it's dropped.
        assert_equal!(configured.nodes["B"].edges_directed, None);
        // A still points at both B and C.
        assert_equal!(
            configured.nodes["A"].edges_directed,
            Some(BTreeSet::from(["B".to_string(), "C".to_string()]))
        );

        Ok(())
    }

    /// The trim applies uniformly to directed, tagged, and dynamic edges.
    /// Force-excluding node F and edge B->C removes F/G/H/I (dynamic subtree
    /// under F) and C (tagged BL target), and drops the directed D->F and tagged
    /// B->(BL) C edges pointing at them.
    #[test]
    fn test_to_configured_map_graph_trims_all_edge_types() -> Result<()> {
        let mut graph = make_test_array_graph_1()?;

        let mut b_edges = BTreeMap::new();
        b_edges.insert("C".to_string(), Decision::exclude());
        let mut force_edges = BTreeMap::new();
        force_edges.insert("B".to_string(), b_edges);
        let mut force_nodes = BTreeMap::new();
        force_nodes.insert("F".to_string(), Decision::exclude());
        let config = TraversalConfig {
            force_nodes: Some(force_nodes),
            force_edges: Some(force_edges),
            force_tagged: None,
            label_predicates: None,
            force_dynamic: None,
            tiered_traversal: None,
            messages: None,
        };
        graph.apply_traversal_config_and_entry_points(config)?;

        let configured_json = serde_json::to_string_pretty(&graph.to_configured_map_graph()?)?;

        snapshot!(
            configured_json,
            r#"
{
  "nodes": {
    "A": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "B",
        "D"
      ]
    },
    "B": {
      "metrics": {
        "size": 1.0
      },
      "edges_tagged": {
        "RD": [
          "J"
        ]
      }
    },
    "D": {
      "metrics": {
        "size": 1.0
      },
      "edges_tagged": {
        "RDFD": [
          "E"
        ]
      }
    },
    "E": {
      "metrics": {
        "size": 1.0
      }
    },
    "J": {
      "labels": {
        "assert_tags": [
          "a",
          "b"
        ]
      },
      "metrics": {
        "size": 1.0
      }
    }
  },
  "traversal_config": null,
  "graph_settings": null,
  "entry_points": null
}
"#
        );
        Ok(())
    }
}

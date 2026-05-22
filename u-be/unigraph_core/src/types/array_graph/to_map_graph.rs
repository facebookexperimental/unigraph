// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::MapGraph;
use crate::NodeIDX;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;

pub fn to_map_graph(graph: &ArrayGraph) -> Result<MapGraph> {
    let mut result = MapGraph {
        nodes: Default::default(),
        traversal_config: graph.runtime.state.traversal_config.clone(),
        graph_settings: graph.data.graph_settings.clone(),
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

fn collect_metrics(graph: &ArrayGraph, node_idx: NodeIDX) -> BTreeMap<String, f32> {
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
    use crate::tests::test_graphs::make_test_array_graph_1;
    use crate::tests::test_graphs::make_test_graph_1;

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
}

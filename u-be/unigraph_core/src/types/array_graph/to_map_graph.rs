// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::MapGraph;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;

pub fn to_map_graph(graph: &ArrayGraph) -> Result<MapGraph> {
    let mut result = MapGraph {
        nodes: Default::default(),
        traversal_config: graph.state.traversal_config.clone(),
        graph_settings: graph.graph_settings.clone(),
        entry_points: graph.entry_points.clone(),
        properties: graph.properties.clone(),
    };

    for node_idx in graph.node_idx_iter() {
        let mut directed = BTreeSet::new();
        for edge in graph.edges_forward.edges(node_idx) {
            if !edge.is_tagged_or_dynamic() {
                directed.insert(graph.idx_to_name(edge.points_to).to_string());
            }
        }

        let tagged = graph.edges_tagged.get(&node_idx).cloned().map(|edges| {
            edges
                .into_iter()
                .map(|(tag, points_to_set)| {
                    (
                        tag,
                        points_to_set
                            .into_iter()
                            .map(|points_to| graph.idx_to_name(points_to).to_string())
                            .collect(),
                    )
                })
                .collect()
        });

        let dynamic = graph.edges_dynamic.get(&node_idx).map(|type_map| {
            type_map
                .iter()
                .map(|(type_key, edge_map)| {
                    let inner = edge_map
                        .iter()
                        .map(|(edge_name, edge)| {
                            (
                                edge_name.clone(),
                                DynamicEdge {
                                    branches: edge
                                        .branches
                                        .iter()
                                        .map(|(branch, pts)| {
                                            (
                                                branch.clone(),
                                                pts.iter()
                                                    .map(|pt| graph.idx_to_name(*pt).to_string())
                                                    .collect(),
                                            )
                                        })
                                        .collect(),
                                    metadata: edge.metadata.clone(),
                                },
                            )
                        })
                        .collect::<BTreeMap<_, _>>();
                    (type_key.clone(), inner)
                })
                .collect::<BTreeMap<_, _>>()
        });

        // Collect labels for this node from the inverted index
        let mut node_labels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (label_name, node_map) in &graph.node_labels {
            if let Some(values) = node_map.get(&node_idx) {
                node_labels.insert(label_name.clone(), values.clone());
            }
        }

        // Collect properties for this node from the inverted index
        let mut node_properties: BTreeMap<String, String> = BTreeMap::new();
        for (prop_name, node_map) in &graph.node_properties {
            if let Some(value) = node_map.get(&node_idx) {
                node_properties.insert(prop_name.clone(), value.clone());
            }
        }

        // Filter out zero-valued metrics for lossless roundtrip
        let metrics: BTreeMap<String, f32> = graph
            .node_metrics
            .iter()
            .filter_map(|(name, values)| {
                let v = values[node_idx];
                if v != 0.0 {
                    Some((name.to_string(), v))
                } else {
                    None
                }
            })
            .collect();

        let map_node = GraphNode {
            properties: if node_properties.is_empty() {
                None
            } else {
                Some(node_properties)
            },
            labels: if node_labels.is_empty() {
                None
            } else {
                Some(node_labels)
            },
            metrics: if metrics.is_empty() {
                None
            } else {
                Some(metrics)
            },
            edges_directed: if directed.is_empty() {
                None
            } else {
                Some(directed)
            },
            edges_tagged: tagged,
            edges_dynamic: dynamic,
        };
        result
            .nodes
            .insert(graph.idx_to_name(node_idx).to_string(), map_node);
    }

    Ok(result)
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

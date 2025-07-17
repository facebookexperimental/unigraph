use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::MapGraph;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;
use crate::types::map_graph::MapGraphEdges;

pub fn to_map_graph(graph: &ArrayGraph) -> Result<MapGraph> {
    let mut result = MapGraph {
        nodes: Default::default(),
        traversal_config: graph.state.traversal_config.clone(),
        graph_settings: graph.graph_settings.clone(),
        entry_points: graph.entry_points.clone(),
    };

    for node_idx in graph.node_idx_iter() {
        let mut directed = BTreeSet::new();
        for edge in graph.edges_forward.edges(node_idx) {
            if !edge.is_tagged_or_dynamic() {
                directed.insert(
                    graph
                        .node_names_ordered
                        .idx_to_name(edge.points_to)
                        .to_string(),
                );
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
                            .map(|points_to| {
                                graph.node_names_ordered.idx_to_name(points_to).to_string()
                            })
                            .collect(),
                    )
                })
                .collect()
        });

        let dynamic = graph.edges_dynamic.get(&node_idx).map(|edges| {
            edges
                .iter()
                .map(|edge| DynamicEdge {
                    properties: edge.properties.clone(),
                    branches: edge
                        .branches
                        .iter()
                        .map(|(branch, points_to_set)| {
                            (
                                branch.clone(),
                                points_to_set
                                    .iter()
                                    .map(|points_to| {
                                        graph.node_names_ordered.idx_to_name(*points_to).to_string()
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<BTreeMap<_, _>>(),
                })
                .collect::<Vec<_>>()
        });

        let edges = MapGraphEdges {
            directed: if directed.is_empty() {
                None
            } else {
                Some(directed)
            },
            tagged,
            dynamic,
        };

        let tag_sets = graph.tag_sets.get(&node_idx).cloned().unwrap_or_default();

        let metrics = graph
            .metrics
            .iter()
            .map(|(name, values)| (name.to_string(), values[node_idx]))
            .collect::<BTreeMap<_, _>>();

        let map_node = GraphNode {
            edges,
            extra_fields: Default::default(), // TODO: we need to do this when we add extra fields support
            tag_sets: if tag_sets.is_empty() {
                None
            } else {
                Some(tag_sets)
            },
            metrics: Some(metrics),
        };
        result.nodes.insert(
            graph.node_names_ordered.idx_to_name(node_idx).to_string(),
            map_node,
        );
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
      "edges": {
        "directed": [
          "B",
          "D"
        ]
      },
      "metrics": {
        "size": 1.0
      }
    },
    "B": {
      "edges": {
        "tagged": {
          "BL": [
            "C"
          ],
          "RD": [
            "J"
          ]
        }
      },
      "metrics": {
        "size": 1.0
      }
    },
    "C": {
      "edges": {},
      "tag_sets": {
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
      "edges": {
        "directed": [
          "F"
        ],
        "tagged": {
          "RDFD": [
            "E"
          ]
        }
      },
      "metrics": {
        "size": 1.0
      }
    },
    "E": {
      "edges": {},
      "metrics": {
        "size": 1.0
      }
    },
    "F": {
      "edges": {
        "dynamic": [
          {
            "properties": {
              "type": "DDD"
            },
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
        ]
      },
      "metrics": {
        "size": 1.0
      }
    },
    "G": {
      "edges": {},
      "metrics": {
        "size": 1.0
      }
    },
    "H": {
      "edges": {},
      "metrics": {
        "size": 1.0
      }
    },
    "I": {
      "edges": {},
      "metrics": {
        "size": 1.0
      }
    },
    "J": {
      "edges": {},
      "tag_sets": {
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;

use crate::ArrayGraph;
use crate::GraphSide;
use crate::MapGraph;
use crate::TraversalConfig;
use crate::TwinGraph;
use crate::tests::test_utils::traversal_config_test_trait::TraversalConfigTestTrait;

// https://fburl.com/excalidraw/vgjzlq2q
const TEST_GRAPH_1: &str = include_str!("./test_graphs/test_graph_1.json");

// Larger graph with more complex cases. Cycles, multiple parents, etc
// https://fburl.com/excalidraw/23gavkrb
const TEST_GRAPH_2: &str = include_str!("./test_graphs/test_graph_2_left.json");

const TEST_GRAPH_2_RIGHT: &str = include_str!("./test_graphs/test_graph_2_right.json");

pub(crate) fn make_test_graph_1() -> MapGraph {
    MapGraph::from_json(TEST_GRAPH_1).unwrap()
}

pub(crate) fn make_test_array_graph_1() -> Result<ArrayGraph> {
    let graph = make_test_graph_1();
    graph.to_array_graph(&ll::Task::create_new("test"))
}

pub(crate) fn make_test_array_graph_2() -> Result<ArrayGraph> {
    MapGraph::from_json(TEST_GRAPH_2)?.to_array_graph(&ll::Task::create_new("test"))
}

pub fn make_twin_graph() -> Result<TwinGraph> {
    let left = MapGraph::from_json(TEST_GRAPH_2)?.to_array_graph_serializable()?;
    let right = MapGraph::from_json(TEST_GRAPH_2_RIGHT)?.to_array_graph_serializable()?;
    TwinGraph::from_two(left, right, &ll::Task::create_new("test"))
}

pub fn make_twin_graph_with_tier_config() -> Result<TwinGraph> {
    let mut tg = make_twin_graph()?;
    let mut tvc = TraversalConfig::default();
    tvc.with_tier_config();

    tg.graph_mut(GraphSide::Left)
        .apply_traversal_config_and_entry_points(tvc.clone())?;
    tg.graph_mut(GraphSide::Right)
        .apply_traversal_config_and_entry_points(tvc)?;
    Ok(tg)
}

#[test]
fn test_graph() {
    let graph = make_test_graph_1();
    snapshot!(
        serde_json::to_string_pretty(&graph).unwrap(),
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
}

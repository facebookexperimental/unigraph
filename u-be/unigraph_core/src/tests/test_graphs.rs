use anyhow::Result;
use k9::snapshot;

use crate::ArrayGraph;
use crate::MapGraph;

// https://fburl.com/excalidraw/vgjzlq2q
const TEST_GRAPH_1: &str = include_str!("./test_graphs/test_graph_1.json");

// Larger graph with more complex cases. Cycles, multiple parents, etc
// https://fburl.com/excalidraw/23gavkrb
const TEST_GRAPH_2: &str = include_str!("./test_graphs/test_graph_2.json");

pub(crate) fn make_test_graph_1() -> MapGraph {
    MapGraph::from_json(TEST_GRAPH_1).unwrap()
}

pub(crate) fn make_test_array_graph_1() -> Result<ArrayGraph> {
    let graph = make_test_graph_1();
    graph.to_array_graph()
}

pub(crate) fn make_test_array_graph_2() -> Result<ArrayGraph> {
    MapGraph::from_json(TEST_GRAPH_2)?.to_array_graph()
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
  "graph_settings": null
}
"#
    );
}

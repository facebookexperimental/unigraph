use anyhow::Result;
use k9::snapshot;

use crate::ArrayGraph;
use crate::AscendingTier;
use crate::AscendingTiersConfig;
use crate::MapGraph;
use crate::TieredTraversalConfig;
use crate::TraversalConfig;

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

pub(crate) fn traversal_config_with_tiers() -> TraversalConfig {
    let tiered_config = AscendingTiersConfig {
        tiers: vec![
            AscendingTier {
                name: "T1".into(),
                tags_that_transition_to_this_tier: vec![],
            },
            AscendingTier {
                name: "T2".into(),
                tags_that_transition_to_this_tier: vec!["RDFD".into()],
            },
            AscendingTier {
                name: "T3".into(),
                tags_that_transition_to_this_tier: vec!["RD".into()],
            },
            AscendingTier {
                name: "T4".into(),
                tags_that_transition_to_this_tier: vec!["BL".into()],
            },
        ],
    };

    TraversalConfig {
        tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(tiered_config.clone())),
        ..Default::default()
    }
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

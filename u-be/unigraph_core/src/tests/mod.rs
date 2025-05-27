// Copyright (c) Meta Platforms, Inc. and affiliates.

// https://fburl.com/excalidraw/vgjzlq2q

mod test_utils;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use k9::assert_equal;
use k9::snapshot;
use maplit::btreemap;
use test_utils::idx_to_names;

use crate::ArrayGraph;
use crate::MapGraph;
use crate::traversal::Decision;
use crate::traversal::TraversalConfig;

const TEST_GRAPH_1: &str = r#"
{
    "nodes": {
        "A": {
            "edges_directed": ["B", "D"]
        },
        "B": {
            "edges_tagged": {"BL": ["C"], "RD": ["J"]}
        },
        "C": {},
        "D": {
            "edges_directed": ["F"],
            "edges_tagged": {"RDFD": ["E"]}
        },
        "E": {},
        "F": {
            "edges_dynamic": [
                {
                    "properties": {"type": "DDD"},
                    "branches": {
                        "b1": ["G", "H"],
                        "b2": ["I"]
                    }
                }
            ]
        },
        "G": {},
        "H": {},
        "I": {},
        "J": {}
    }
}
    "#;

pub(crate) fn make_test_graph() -> MapGraph {
    MapGraph::from_json(TEST_GRAPH_1).unwrap()
}

pub(crate) fn make_test_array_graph() -> Result<ArrayGraph> {
    let graph = make_test_graph();
    graph.to_array_graph()
}

#[test]
fn determine_entrypoints() -> Result<()> {
    let g = make_test_array_graph()?;
    let entrypoints = g.idxs_to_names(&g.determine_entrypoints());
    assert_equal!(entrypoints, vec!["A"]);
    Ok(())
}

#[test]
fn test_edge_flags() -> Result<()> {
    let g = make_test_array_graph()?;

    let get_flags = |node_name: &str| {
        g.edges_forward
            .edges(g.node_names.name_to_idx_log(node_name).unwrap())
            .into_iter()
            .map(|e| {
                format!(
                    "{} -> {}: {}",
                    node_name,
                    g.node_names.idx_to_name(e.points_to),
                    e.flags.to_binary_string()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let result = g
        .node_idx_iter()
        .map(|idx| get_flags(g.idx_to_name(idx)))
        .collect::<Vec<_>>()
        .join("\n");
    snapshot!(
        result.trim(),
        "
A -> B: 0000_0000_0000_0000
A -> D: 0000_0000_0000_0000
B -> C: 0000_0001_0000_0000
B -> J: 0000_0001_0000_0000

D -> F: 0000_0000_0000_0000
D -> E: 0000_0001_0000_0000

F -> G: 0000_0010_0000_0000
F -> H: 0000_0010_0000_0000
F -> I: 0000_0010_0000_0000
"
    );

    Ok(())
}

#[test]
fn test_arrows() -> Result<()> {
    let g = make_test_array_graph()?;
    let mut result = String::new();
    for node_idx in g.node_idx_iter() {
        let node_name = g.idx_to_name(node_idx);
        result.push_str(&format!("{}:\n", node_name));
        for arrow in g.get_arrows_forward(node_idx)? {
            result.push_str(
                format!(
                    "({}, {}, {}) -> {}\n",
                    serde_json::to_string(&arrow.tag)?,
                    serde_json::to_string(&arrow.branch)?,
                    serde_json::to_string(&arrow.properties)?,
                    g.idx_to_name(arrow.points_to)
                )
                .as_str(),
            );
        }

        result.push_str("---------------------------------------------\n");
    }

    snapshot!(
        result,
        r#"
A:
(null, null, null) -> B
(null, null, null) -> D
---------------------------------------------
B:
("BL", null, null) -> C
("RD", null, null) -> J
---------------------------------------------
C:
---------------------------------------------
D:
(null, null, null) -> F
("RDFD", null, null) -> E
---------------------------------------------
E:
---------------------------------------------
F:
(null, "b1", {"type":"DDD"}) -> G
(null, "b1", {"type":"DDD"}) -> H
(null, "b2", {"type":"DDD"}) -> I
---------------------------------------------
G:
---------------------------------------------
H:
---------------------------------------------
I:
---------------------------------------------
J:
---------------------------------------------

"#
    );

    Ok(())
}

#[test]
fn test_edges_iter() -> Result<()> {
    let mut g = make_test_array_graph()?;
    let g2 = make_test_array_graph()?;

    let snap = |g: &mut ArrayGraph| {
        let mut result = String::new();

        for (parent_idx, edge, metadata) in g.edges_forward.iter_edges_mut() {
            result.push_str(&format!(
                "{} -> {}: {} ({:?})\n",
                g2.idx_to_name(parent_idx),
                g2.idx_to_name(edge.points_to),
                edge.flags.to_binary_string(),
                metadata
            ));
        }
        result.trim().to_string()
    };

    snapshot!(
        snap(&mut g),
        r#"
A -> B: 0000_0000_0000_0000 (Directed)
A -> D: 0000_0000_0000_0000 (Directed)
B -> C: 0000_0001_0000_0000 (Tagged { tag: "BL" })
B -> J: 0000_0001_0000_0000 (Tagged { tag: "RD" })
D -> F: 0000_0000_0000_0000 (Directed)
D -> E: 0000_0001_0000_0000 (Tagged { tag: "RDFD" })
F -> G: 0000_0010_0000_0000 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> H: 0000_0010_0000_0000 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> I: 0000_0010_0000_0000 (Dynamic { properties: {"type": "DDD"}, branch: "b2" })
"#
    );

    g.apply_traversal_config(&TraversalConfig {
        force_nodes: btreemap! { "I".into() => Decision { follow: false, message: None } },
        force_edges: BTreeMap::new(),
    })?;

    snapshot!(
        snap(&mut g),
        r#"
A -> B: 0000_0000_0000_0000 (Directed)
A -> D: 0000_0000_0000_0000 (Directed)
B -> C: 0000_0001_0000_0000 (Tagged { tag: "BL" })
B -> J: 0000_0001_0000_0000 (Tagged { tag: "RD" })
D -> F: 0000_0000_0000_0000 (Directed)
D -> E: 0000_0001_0000_0000 (Tagged { tag: "RDFD" })
F -> G: 0000_0010_0000_0000 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> H: 0000_0010_0000_0000 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> I: 0000_0110_0000_0000 (Dynamic { properties: {"type": "DDD"}, branch: "b2" })
"#
    );
    Ok(())
}

#[test]
fn test_dfs_with_traversal_config() -> Result<()> {
    let mut g = make_test_array_graph()?;
    let traversal_config = TraversalConfig {
        force_nodes: btreemap! { "D".into() => Decision { follow: false, message: None } },
        force_edges: btreemap! { "B".into() => btreemap! { "C".into() => Decision { follow: false, message: None } } },
    };

    let dfs = |g: &ArrayGraph| {
        let mut visited = BTreeSet::new();
        for node in g.edges_forward.dfs(&g.determine_entrypoints()) {
            visited.insert(node);
        }
        visited
    };

    snapshot!(idx_to_names(&g, dfs(&g)).join(" "), "A B C D E F G H I J");

    g.apply_traversal_config(&traversal_config)?;

    snapshot!(idx_to_names(&g, dfs(&g)).join(" "), "A B J");
    Ok(())
}

#[test]
fn test_graph() {
    let graph = make_test_graph();
    snapshot!(
        serde_json::to_string_pretty(&graph).unwrap(),
        r#"
{
  "nodes": {
    "A": {
      "edges_directed": [
        "B",
        "D"
      ]
    },
    "B": {
      "edges_tagged": {
        "BL": [
          "C"
        ],
        "RD": [
          "J"
        ]
      }
    },
    "C": {},
    "D": {
      "edges_directed": [
        "F"
      ],
      "edges_tagged": {
        "RDFD": [
          "E"
        ]
      }
    },
    "E": {},
    "F": {
      "edges_dynamic": [
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
    "G": {},
    "H": {},
    "I": {},
    "J": {}
  }
}
"#
    );
}

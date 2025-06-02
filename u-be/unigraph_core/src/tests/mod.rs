// Copyright (c) Meta Platforms, Inc. and affiliates.

// https://fburl.com/excalidraw/vgjzlq2q

mod test_utils;

use std::collections::BTreeSet;

use anyhow::Result;
use k9::assert_equal;
use k9::snapshot;
use maplit::btreemap;
use test_utils::idx_to_names;

use crate::ArrayGraph;
use crate::MapGraph;
use crate::traversal::Decision;
use crate::traversal::ForceDynamic;
use crate::traversal::NodeTagSetsPredicate;
use crate::traversal::TraversalConfig;
use crate::traversal::tiered_traversal::AscendingTier;
use crate::traversal::tiered_traversal::AscendingTiersConfig;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::array_graph::NodeFlags;

const TEST_GRAPH_1: &str = r#"
{
    "nodes": {
        "A": {
            "edges": {"directed": ["B", "D"]}
        },
        "B": {
            "edges": {"tagged": {"BL": ["C"], "RD": ["J"]}}
        },
        "C": {
          "tag_sets": {"disallow_tags": ["b", "c"]}
        },
        "D": {
            "edges": {"directed": ["F"], "tagged": {"RDFD": ["E"]}}
        },
        "E": {},
        "F": {
            "edges": {
                "dynamic": [
                    {
                        "properties": {"type": "DDD"},
                        "branches": {
                            "b1": ["G", "H"],
                            "b2": ["I"]
                        }
                    }
                ]
            }
        },
        "G": {},
        "H": {},
        "I": {},
        "J": {
          "tag_sets": {"assert_tags": ["a", "b"]}
        }
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
B -> C: 0000_0000_0000_0001
B -> J: 0000_0000_0000_0001

D -> F: 0000_0000_0000_0000
D -> E: 0000_0000_0000_0001

F -> G: 0000_0000_0000_0010
F -> H: 0000_0000_0000_0010
F -> I: 0000_0000_0000_0010
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
B -> C: 0000_0000_0000_0001 (Tagged { tag: "BL" })
B -> J: 0000_0000_0000_0001 (Tagged { tag: "RD" })
D -> F: 0000_0000_0000_0000 (Directed)
D -> E: 0000_0000_0000_0001 (Tagged { tag: "RDFD" })
F -> G: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> H: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> I: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b2" })
"#
    );

    g.apply_traversal_config(&TraversalConfig {
        force_nodes: btreemap! { "I".into() => Decision { include: false, message: None } },
        ..Default::default()
    })?;

    snapshot!(
        snap(&mut g),
        r#"
A -> B: 0000_0000_0000_0000 (Directed)
A -> D: 0000_0000_0000_0000 (Directed)
B -> C: 0000_0000_0000_0001 (Tagged { tag: "BL" })
B -> J: 0000_0000_0000_0001 (Tagged { tag: "RD" })
D -> F: 0000_0000_0000_0000 (Directed)
D -> E: 0000_0000_0000_0001 (Tagged { tag: "RDFD" })
F -> G: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> H: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> I: 0000_0000_0000_0110 (Dynamic { properties: {"type": "DDD"}, branch: "b2" })
"#
    );
    Ok(())
}

#[test]
fn test_dfs_with_traversal_config() -> Result<()> {
    let mut g = make_test_array_graph()?;
    let traversal_config = TraversalConfig {
        force_nodes: btreemap! { "D".into() => Decision { include: false, message: None } },
        force_edges: btreemap! { "B".into() => btreemap! { "C".into() => Decision { include: false, message: None } } },
        ..Default::default()
    };

    snapshot!(dfs(&g), "A B C D E F G H I J");

    g.apply_traversal_config(&traversal_config)?;

    snapshot!(dfs(&g), "A B J");
    Ok(())
}

fn dfs(g: &ArrayGraph) -> String {
    let mut visited = BTreeSet::new();
    for node in g.edges_forward.dfs(&g.determine_entrypoints()) {
        visited.insert(node);
    }
    idx_to_names(&g, visited).join(" ")
}

#[test]
fn test_dfs_with_traversal_config_on_dynamic_edges() -> Result<()> {
    let mut g = make_test_array_graph()?;
    let mut traversal_config = TraversalConfig::default();

    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B C D E F G H I J");

    traversal_config.force_dynamic = vec![ForceDynamic {
        from_node: Some("F".into()),
        match_properties: btreemap! { "type".into() => "DDD".into() },
        branch: Some("b1".into()),
        decision: Decision {
            include: false,
            message: None,
        },
    }];

    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B C D E F I J");

    traversal_config.force_dynamic = vec![ForceDynamic {
        from_node: None,
        match_properties: btreemap! { "type".into() => "DDD".into() },
        branch: Some("b2".into()),
        decision: Decision {
            include: false,
            message: None,
        },
    }];

    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B C D E F G H J");

    // Set a default branch to follow for DDD
    traversal_config.force_dynamic = vec![
        ForceDynamic {
            from_node: None,
            match_properties: btreemap! { "type".into() => "DDD".into() },
            branch: Some("b2".into()),
            decision: Decision {
                include: true,
                message: None,
            },
        },
        ForceDynamic {
            from_node: None,
            match_properties: btreemap! { "type".into() => "DDD".into() },
            branch: None,
            decision: Decision {
                include: false,
                message: None,
            },
        },
    ];

    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B C D E F I J");

    // follow nothing
    traversal_config.force_dynamic = vec![ForceDynamic {
        from_node: None,
        match_properties: btreemap! {},
        branch: None,
        decision: Decision {
            include: false,
            message: None,
        },
    }];

    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B C D E F J");

    Ok(())
}

#[test]
fn test_dfs_with_traversal_config_tag_sets() -> Result<()> {
    let mut g = make_test_array_graph()?;
    let mut traversal_config = TraversalConfig::default();

    let set_global_value = |tc: &mut TraversalConfig, value: &str| {
        tc.tag_sets = vec![
            NodeTagSetsPredicate {
                tag_set_name: "assert_tags".into(),
                tag_name: value.into(),
                contains: false,
                decision: Decision::exclude(),
            },
            NodeTagSetsPredicate {
                tag_set_name: "assert_tags".into(),
                tag_name: value.into(),
                contains: true,
                decision: Decision::include(),
            },
            NodeTagSetsPredicate {
                tag_set_name: "disallow_tags".into(),
                tag_name: value.into(),
                contains: true,
                decision: Decision::exclude(),
            },
            NodeTagSetsPredicate {
                tag_set_name: "disallow_tags".into(),
                tag_name: value.into(),
                contains: false,
                decision: Decision::include(),
            },
        ];
    };

    set_global_value(&mut traversal_config, "a");
    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B C D E F G H I J");

    set_global_value(&mut traversal_config, "b");
    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B D E F G H I J");

    set_global_value(&mut traversal_config, "c");
    g.apply_traversal_config(&traversal_config)?;
    snapshot!(dfs(&g), "A B D E F G H I");

    Ok(())
}

#[test]
fn tiered_traversal() -> Result<()> {
    let mut g = make_test_array_graph()?;
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

    let traversal_config = TraversalConfig {
        tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(tiered_config.clone())),
        ..Default::default()
    };

    g.apply_traversal_config(&traversal_config)?;

    let mut result = String::new();
    for node_idx in g.node_idx_iter() {
        let node_name = g.idx_to_name(node_idx);
        let tier_flags = g.node_flags[node_idx].intersection(NodeFlags::ALL_TIERS);
        let tier_idx = match tier_flags {
            NodeFlags::TIER_0 => 0,
            NodeFlags::TIER_1 => 1,
            NodeFlags::TIER_2 => 2,
            NodeFlags::TIER_3 => 3,
            NodeFlags::TIER_4 => 4,
            NodeFlags::TIER_5 => 5,
            NodeFlags::TIER_6 => 6,
            NodeFlags::TIER_7 => 7,
            _ => anyhow::bail!("Does not match any tier or has multiple tiers assigned"), // No tier assigned
        };

        result.push_str(&format!(
            "Node: {}, Tier: {}\n",
            node_name, tiered_config.tiers[tier_idx].name
        ));
    }

    snapshot!(
        result.trim(),
        "
Node: A, Tier: T1
Node: B, Tier: T1
Node: C, Tier: T4
Node: D, Tier: T1
Node: E, Tier: T2
Node: F, Tier: T1
Node: G, Tier: T1
Node: H, Tier: T1
Node: I, Tier: T1
Node: J, Tier: T3
"
    );

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
      "edges": {
        "directed": [
          "B",
          "D"
        ]
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
      }
    },
    "C": {
      "edges": {},
      "tag_sets": {
        "disallow_tags": [
          "b",
          "c"
        ]
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
      }
    },
    "E": {
      "edges": {}
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
      }
    },
    "G": {
      "edges": {}
    },
    "H": {
      "edges": {}
    },
    "I": {
      "edges": {}
    },
    "J": {
      "edges": {},
      "tag_sets": {
        "assert_tags": [
          "a",
          "b"
        ]
      }
    }
  }
}
"#
    );
}

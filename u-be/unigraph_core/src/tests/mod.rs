// Copyright (c) Meta Platforms, Inc. and affiliates.

pub(crate) mod test_graphs;
pub(crate) mod test_utils;
mod traversal_test;

use std::collections::BTreeSet;

use anyhow::Result;
use k9::assert_equal;
use k9::snapshot;
use maplit::btreemap;
use test_utils::idx_to_names;
use test_utils::print_all_node_names;
use test_utils::print_forward_edges;

use crate::ArrayGraph;
use crate::ArrayGraphDebugUtils;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::tests::test_graphs::make_test_array_graph_1;
use crate::tests::test_graphs::make_test_array_graph_2;
use crate::tests::test_utils::print_arrows;
use crate::tests::test_utils::traversal_config_test_trait::TraversalConfigTestTrait;
use crate::traversal::Decision;
use crate::traversal::ForceDynamic;
use crate::traversal::NodeTagSetsPredicate;
use crate::traversal::TraversalConfig;
use crate::types::array_graph::NodeFlags;

#[test]
fn determine_entrypoints() -> Result<()> {
    let g = make_test_array_graph_1()?;
    let entrypoints = g.idxs_to_names(&g.determine_entrypoints());
    assert_equal!(entrypoints, vec!["A"]);
    Ok(())
}

#[test]
fn test_edge_flags() -> Result<()> {
    let g = make_test_array_graph_1()?;

    let get_flags = |node_name: &str| {
        g.edges_forward
            .edges(g.nodes.name_to_idx_log(node_name).unwrap())
            .iter()
            .map(|e| {
                format!(
                    "{} -> {}: {}",
                    node_name,
                    g.idx_to_name(e.points_to),
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
    let g = make_test_array_graph_1()?;
    let mut result = String::new();
    for node_idx in g.node_idx_iter() {
        let node_name = g.idx_to_name(node_idx);
        result.push_str(&format!("{node_name}:\n"));
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
    let mut g = make_test_array_graph_1()?;
    let g2 = make_test_array_graph_1()?;

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
        print_arrows(&g),
        r#"
A -> B
A -> D
B -> C
   tag: BL
B -> J
   tag: RD
D -> F
D -> E
   tag: RDFD
F -> G
   branch: b1
   properties: {"type": "DDD"}
F -> H
   branch: b1
   properties: {"type": "DDD"}
F -> I
   branch: b2
   properties: {"type": "DDD"}
"#
    );

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

    g.apply_traversal_config(TraversalConfig {
        force_nodes: Some(
            btreemap! { "I".into() => Decision { include: false, message_id: None } },
        ),
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
F -> I: 0000_1100_0000_0110 (Dynamic { properties: {"type": "DDD"}, branch: "b2" })
"#
    );

    g.apply_traversal_config(TraversalConfig {
        force_tagged: Some(
            btreemap! { "BL".into() => Decision { include: false, message_id: None } },
        ),
        ..Default::default()
    })?;

    snapshot!(
        snap(&mut g),
        r#"
A -> B: 0000_0000_0000_0000 (Directed)
A -> D: 0000_0000_0000_0000 (Directed)
B -> C: 0000_1000_0000_0101 (Tagged { tag: "BL" })
B -> J: 0000_0000_0000_0001 (Tagged { tag: "RD" })
D -> F: 0000_0000_0000_0000 (Directed)
D -> E: 0000_0000_0000_0001 (Tagged { tag: "RDFD" })
F -> G: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> H: 0000_0000_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b1" })
F -> I: 0000_1100_0000_0010 (Dynamic { properties: {"type": "DDD"}, branch: "b2" })
"#
    );
    Ok(())
}

#[test]
fn test_dfs_with_traversal_config() -> Result<()> {
    let mut g = make_test_array_graph_1()?;
    let traversal_config = TraversalConfig {
        force_nodes: Some(
            btreemap! { "D".into() => Decision { include: false, message_id: None } },
        ),
        force_edges: Some(
            btreemap! { "B".into() => btreemap! { "C".into() => Decision { include: false, message_id: None } } },
        ),
        ..Default::default()
    };

    snapshot!(dfs_configured(&g), "A B C D E F G H I J");

    g.apply_traversal_config(traversal_config)?;

    snapshot!(dfs_configured(&g), "A B J");
    Ok(())
}

fn dfs_configured(g: &ArrayGraph) -> String {
    let mut visited = BTreeSet::new();
    for node in g.edges_forward.dfs_configured(&g.determine_entrypoints()) {
        visited.insert(node);
    }
    idx_to_names(g, visited).join(" ")
}

fn dfs_unconfigured(g: &ArrayGraph) -> String {
    let mut visited = BTreeSet::new();
    for node in g.edges_forward.dfs_unconfigured(&g.determine_entrypoints()) {
        visited.insert(node);
    }
    idx_to_names(g, visited).join(" ")
}

#[test]
fn test_dfs_with_traversal_config_on_dynamic_edges() -> Result<()> {
    let mut g = make_test_array_graph_1()?;
    let mut traversal_config = TraversalConfig::default();

    g.apply_traversal_config(traversal_config.clone())?;
    snapshot!(dfs_configured(&g), "A B C D E F G H I J");

    traversal_config.force_dynamic = Some(vec![ForceDynamic {
        from_node: Some("F".into()),
        match_properties: btreemap! { "type".into() => "DDD".into() },
        branch: Some("b1".into()),
        decision: Decision {
            include: false,
            message_id: None,
        },
    }]);

    g.apply_traversal_config(traversal_config.clone())?;
    snapshot!(dfs_configured(&g), "A B C D E F I J");

    traversal_config.force_dynamic = Some(vec![ForceDynamic {
        from_node: None,
        match_properties: btreemap! { "type".into() => "DDD".into() },
        branch: Some("b2".into()),
        decision: Decision {
            include: false,
            message_id: None,
        },
    }]);

    g.apply_traversal_config(traversal_config.clone())?;
    snapshot!(dfs_configured(&g), "A B C D E F G H J");

    // Set a default branch to follow for DDD
    traversal_config.force_dynamic = Some(vec![
        ForceDynamic {
            from_node: None,
            match_properties: btreemap! { "type".into() => "DDD".into() },
            branch: Some("b2".into()),
            decision: Decision {
                include: true,
                message_id: None,
            },
        },
        ForceDynamic {
            from_node: None,
            match_properties: btreemap! { "type".into() => "DDD".into() },
            branch: None,
            decision: Decision {
                include: false,
                message_id: None,
            },
        },
    ]);

    g.apply_traversal_config(traversal_config.clone())?;
    snapshot!(dfs_configured(&g), "A B C D E F I J");

    // follow nothing
    traversal_config.force_dynamic = Some(vec![ForceDynamic {
        from_node: None,
        match_properties: btreemap! {},
        branch: None,
        decision: Decision {
            include: false,
            message_id: None,
        },
    }]);

    g.apply_traversal_config(traversal_config)?;
    snapshot!(dfs_configured(&g), "A B C D E F J");

    Ok(())
}

#[test]
fn test_dfs_with_traversal_config_tag_sets() -> Result<()> {
    let mut g = make_test_array_graph_1()?;
    let mut traversal_config = TraversalConfig::default();

    let set_global_value = |tc: &mut TraversalConfig, value: &str| {
        tc.tag_sets = Some(vec![
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
        ]);
    };

    set_global_value(&mut traversal_config, "a");
    g.apply_traversal_config(traversal_config.clone())?;
    snapshot!(dfs_configured(&g), "A B C D E F G H I J");

    set_global_value(&mut traversal_config, "b");
    g.apply_traversal_config(traversal_config.clone())?;
    snapshot!(dfs_configured(&g), "A B D E F G H I J");

    set_global_value(&mut traversal_config, "c");
    g.apply_traversal_config(traversal_config)?;
    snapshot!(dfs_configured(&g), "A B D E F G H I");
    snapshot!(dfs_unconfigured(&g), "A B C D E F G H I J");

    snapshot!(
        g.to_forward_edges_string()?,
        "
A:
  - B
  - D
B:
  - C [T]
  - J [T]
C [UNREACHABLE] (tag sets: disallow_tags: [b, c]):
D:
  - F
  - E [T]
E:
F:
  - G [D]
  - H [D]
  - I [D]
G:
H:
I:
J [UNREACHABLE] (tag sets: assert_tags: [a, b]):
"
    );

    Ok(())
}

#[test]
fn test_tiered_traversal() -> Result<()> {
    let mut g = make_test_array_graph_1()?;
    let mut traversal_config = TraversalConfig::default();
    traversal_config.with_tier_config();
    let tiered_config = traversal_config.get_tier_config();

    g.apply_traversal_config(traversal_config)?;

    let mut result = String::new();
    for node_idx in g.node_idx_iter() {
        let node_name = g.idx_to_name(node_idx);
        let tier_flags = g.node_flags[node_idx].intersection(NodeFlags::ALL_TIERS);
        let tier_idx = match tier_flags {
            NodeFlags::TIER_IDX_0 => 0,
            NodeFlags::TIER_IDX_1 => 1,
            NodeFlags::TIER_IDX_2 => 2,
            NodeFlags::TIER_IDX_3 => 3,
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

    snapshot!(
        g.get_transitive_tiered_metric_values(NodeIDX(0), "size", false)?,
        r#"
{
    "T1": 7.0,
    "T2": 8.0,
    "T3": 9.0,
    "T4": 10.0,
}
"#
    );

    snapshot!(
        g.get_combined_metrics_for_entry_points(None)?,
        r#"
CombinedMetricsForNodes {
    metrics: {
        "size": 10.0,
    },
    tiered_metrics: {
        "size": {
            "T1": 7.0,
            "T2": 8.0,
            "T3": 9.0,
            "T4": 10.0,
        },
    },
    node_count: 10,
}
"#
    );

    Ok(())
}

#[test]
fn test_serializable() -> Result<()> {
    let g = make_test_array_graph_1()?;
    let original_names = print_all_node_names(&g);
    let original_edges = print_forward_edges(&g);
    let s = g.into_serializable();
    let json_zstd = s.to_json()?;
    let original: ArrayGraphSerializable = ArrayGraphSerializable::from_json(&json_zstd)?;
    let roundtrip = original.into_array_graph();
    let rountrip_names = print_all_node_names(&roundtrip);
    let roundtrip_edges = print_forward_edges(&roundtrip);

    snapshot!(&rountrip_names, "A B C D E F G H I J");
    snapshot!(
        &original_edges,
        "
A -> B
A -> D
B -> C [T]
B -> J [T]
D -> F
D -> E [T]
F -> G [D]
F -> H [D]
F -> I [D]
"
    );
    snapshot!(
        &roundtrip_edges,
        "
A -> B
A -> D
B -> C [T]
B -> J [T]
D -> F
D -> E [T]
F -> G [D]
F -> H [D]
F -> I [D]
"
    );

    assert_equal!(
        k9::MultilineString(original_names),
        k9::MultilineString(rountrip_names)
    );
    assert_equal!(
        k9::MultilineString(original_edges),
        k9::MultilineString(roundtrip_edges)
    );
    Ok(())
}

#[test]
fn test_edges_len() -> Result<()> {
    let mut g = make_test_array_graph_2()?;
    let node_k = g.nodes.name_to_idx_log("K").unwrap();

    assert_equal!(g.parents_len_configured(node_k), 2);

    g.apply_traversal_config(TraversalConfig {
        force_nodes: Some(
            btreemap! { "D".into() => Decision { include: false, message_id: None } },
        ),
        ..Default::default()
    })?;

    assert_equal!(g.parents_len_configured(node_k), 1);

    g.apply_traversal_config(TraversalConfig {
        force_nodes: Some(btreemap! {
          "J".into() => Decision { include: false, message_id: None },
          "D".into() => Decision { include: false, message_id: None }
        }),
        ..Default::default()
    })?;

    assert_equal!(g.parents_len_configured(node_k), 0);

    Ok(())
}

#[test]
fn test_reacable_subgraph_unconfigured() -> Result<()> {
    let g = make_test_array_graph_1()?;
    let d = g.nodes.name_to_idx_log("D").unwrap();
    let sg = g.get_reachable_subgraph_unconfigured(&[d])?;
    let reachable = sg.into_array_graph();
    snapshot!(
        reachable.to_forward_edges_string()?,
        "
D:
  - F
  - E [T]
E:
F:
  - G [D]
  - H [D]
  - I [D]
G:
H:
I:
"
    );
    Ok(())
}

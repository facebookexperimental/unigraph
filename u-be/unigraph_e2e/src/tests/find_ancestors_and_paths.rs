// Copyright (c) Meta Platforms, Inc. and affiliates.

//! E2E tests for `FindAncestors` and `FindPath` RPCs.
//!
//! Tests the full flow: ingest a graph with properties, find ancestors by
//! node selection or parentless, then find shortest paths back.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::FindAncestorsInput;
use unigraph_app::FindPathInput;
use unigraph_app::GraphHandle;
use unigraph_app::call_rpc;
use unigraph_core::NodeSelection;

use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── FindAncestors Tests ─────────────────────────────────────

#[tokio::test]
async fn find_ancestors_by_parentless() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "utils".to_string(),
            selection: NodeSelection::default(),
            parentless: Some(true),
            offset: None,
            limit: None,
            include_ascii: Some(true),
        })
    );

    // "app" is the only entrypoint, and it can reach "utils"
    snapshot!(
        out.ascii.unwrap(),
        r#"
Found 1 ancestors of "utils" matching {parentless}:

  1. app

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_ancestors_no_match() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "utils".to_string(),
            selection: NodeSelection::by_property("type", Some("nonexistent".to_string())),
            parentless: None,
            offset: None,
            limit: None,
            include_ascii: None,
        })
    );

    assert_eq!(out.total_count, 0);
    assert!(out.ancestors.is_empty());

    Ok(())
}

#[tokio::test]
async fn find_ancestors_by_properties() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "leaf".to_string(),
            selection: NodeSelection::by_property("type", Some("budget".to_string())),
            parentless: None,
            offset: None,
            limit: None,
            include_ascii: Some(true),
        })
    );

    snapshot!(
        out.ascii.unwrap(),
        r#"
Found 2 ancestors of "leaf" matching {type=budget}:

  1. budget_a
  2. budget_b

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_ancestors_parentless_and_properties() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    // budget_a is parentless (entrypoint), budget_b is not
    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "leaf".to_string(),
            selection: NodeSelection::by_property("type", Some("budget".to_string())),
            parentless: Some(true),
            offset: None,
            limit: None,
            include_ascii: Some(true),
        })
    );

    snapshot!(
        out.ascii.unwrap(),
        r#"
Found 1 ancestors of "leaf" matching {type=budget, parentless}:

  1. budget_a

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_ancestors_pagination() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "leaf".to_string(),
            selection: NodeSelection::by_property("type", Some("budget".to_string())),
            parentless: None,
            offset: Some(0),
            limit: Some(1),
            include_ascii: Some(true),
        })
    );

    assert_eq!(out.total_count, 2);
    assert_eq!(out.ancestors.len(), 1);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Found 2 ancestors of "leaf" matching {type=budget}:

  1. budget_a

(showing 1 of 2 results, offset 0)
"#
    );

    Ok(())
}

// ── FindPath Tests ──────────────────────────────────────────

#[tokio::test]
async fn find_path_simple() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "app".to_string(),
            to: "utils".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(out.found);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Shortest path from "app" to "utils" (1 steps):

app ->
utils

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_path_with_tagged_edge() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "ui".to_string(),
            to: "dialogs".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(out.found);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Shortest path from "ui" to "dialogs" (1 steps):

ui [tag: lazy] ->
dialogs

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_path_with_dynamic_edge() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "components".to_string(),
            to: "button_ios".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(out.found);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Shortest path from "components" to "button_ios" (1 steps):

components [platform:button/ios] ->
button_ios

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_path_no_path() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "utils".to_string(),
            to: "app".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(!out.found);
    assert!(out.path.is_empty());
    snapshot!(out.ascii.unwrap(), r#"No path from "utils" to "app"."#);

    Ok(())
}

#[tokio::test]
async fn find_path_multi_hop() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_explore_graph(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "app".to_string(),
            to: "button_ios".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(out.found);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Shortest path from "app" to "button_ios" (3 steps):

app ->
ui ->
components [platform:button/ios] ->
button_ios

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_path_through_tagged_and_dynamic() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    // route_a -[tag: lazy]-> hub -[platform:widget/ios]-> ios_impl -> leaf
    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "route_a".to_string(),
            to: "ios_impl".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(out.found);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Shortest path from "route_a" to "ios_impl" (2 steps):

route_a [tag: lazy] ->
hub [platform:widget/ios] ->
ios_impl

"#
    );

    Ok(())
}

#[tokio::test]
async fn find_path_with_cycle_does_not_hang() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    // cycle_a -> cycle_b -> cycle_a (cycle), but cycle_a -> leaf exists
    let out = call_rpc!(
        t,
        FindPath(FindPathInput {
            handle: handle.clone(),
            from: "cycle_a".to_string(),
            to: "leaf".to_string(),
            include_ascii: Some(true),
        })
    );

    assert!(out.found);
    snapshot!(
        out.ascii.unwrap(),
        r#"
Shortest path from "cycle_a" to "leaf" (1 steps):

cycle_a ->
leaf

"#
    );

    Ok(())
}

// ── Full Flow: FindAncestors + FindPath ─────────────────────

#[tokio::test]
async fn ancestors_then_paths() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    // Step 1: Find all budget ancestors of "leaf"
    let ancestors = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "leaf".to_string(),
            selection: NodeSelection::by_property("type", Some("budget".to_string())),
            parentless: None,
            offset: None,
            limit: None,
            include_ascii: None,
        })
    );

    assert_eq!(ancestors.ancestors, vec!["budget_a", "budget_b"]);

    // Step 2: Find path from each budget ancestor to the leaf
    let mut paths_ascii = Vec::new();
    for ancestor in &ancestors.ancestors {
        let path = call_rpc!(
            t,
            FindPath(FindPathInput {
                handle: handle.clone(),
                from: ancestor.clone(),
                to: "leaf".to_string(),
                include_ascii: Some(true),
            })
        );
        assert!(path.found);
        paths_ascii.push(path.ascii.unwrap());
    }

    snapshot!(
        paths_ascii.join("---\n"),
        r#"
Shortest path from "budget_a" to "leaf" (2 steps):

budget_a ->
middle ->
leaf
---
Shortest path from "budget_b" to "leaf" (1 steps):

budget_b ->
leaf

"#
    );

    Ok(())
}

#[tokio::test]
async fn parentless_ancestors_then_paths() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_properties(&t).await?.parse()?;

    // Step 1: Find parentless ancestors of "leaf"
    let ancestors = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "leaf".to_string(),
            selection: NodeSelection::default(),
            parentless: Some(true),
            offset: None,
            limit: None,
            include_ascii: None,
        })
    );

    // budget_a, other, and route_a are parentless entrypoints that reach leaf
    // (cycle_a is NOT parentless — cycle_b points to it)
    let expected: Vec<&str> = ancestors.ancestors.iter().map(|s| s.as_str()).collect();
    assert_eq!(expected, vec!["budget_a", "other", "route_a"]);

    Ok(())
}

/// The enum-metric case: pick the ancestor that belongs to one package.
///
/// Also pins the rounding contract. `rounds_up` stores 5.7, which
/// `MetricFormat::format_value` would label as variant 6 — so a `package=6`
/// condition has to match it, or the predicate would disagree with the number
/// the table renders next to it.
#[tokio::test]
async fn find_ancestors_by_enum_metric() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_metrics(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "shared".to_string(),
            selection: NodeSelection::by_metric("package", 6),
            parentless: None,
            offset: None,
            limit: None,
            include_ascii: Some(true),
        })
    );

    snapshot!(
        out.ascii.expect("include_ascii was asked for"),
        r#"
Found 2 ancestors of "shared" matching {package=6}:

  1. prod_root
  2. rounds_up

"#
    );

    Ok(())
}

/// Negative case: a metric the graph does not carry can never match, rather
/// than being quietly ignored and returning every ancestor.
#[tokio::test]
async fn find_ancestors_by_unknown_metric_matches_nothing() -> Result<()> {
    let t = init_app();
    let handle: GraphHandle = ingest_with_metrics(&t).await?.parse()?;

    let out = call_rpc!(
        t,
        FindAncestors(FindAncestorsInput {
            handle: handle.clone(),
            node_name: "shared".to_string(),
            selection: NodeSelection::by_metric("no_such_metric", 6),
            parentless: None,
            offset: None,
            limit: None,
            include_ascii: None,
        })
    );

    assert_eq!(out.total_count, 0);
    assert!(out.ancestors.is_empty());

    Ok(())
}

// ── Fixtures ────────────────────────────────────────────────

/// Three roots into one shared node, told apart by an enum-formatted `package`
/// metric — the shape of the real Hack graph, where 6 is the `prod` package.
///
/// ```text
///   prod_root   (package 6)   ──┐
///   intern_root (package 5)   ──┼──> shared
///   rounds_up   (package 5.7) ──┘
/// ```
///
/// `rounds_up` is the interesting one: stored fractional, it renders as variant
/// 6 and so has to match `package=6`.
async fn ingest_with_metrics(t: &crate::support::app::TestApp) -> Result<String> {
    let json = r#"{
        "nodes": {
            "prod_root": {
                "metrics": {"package": 6},
                "edges_directed": ["shared"]
            },
            "intern_root": {
                "metrics": {"package": 5},
                "edges_directed": ["shared"]
            },
            "rounds_up": {
                "metrics": {"package": 5.7},
                "edges_directed": ["shared"]
            },
            "shared": {}
        }
    }"#;
    crate::support::fixtures::ingest_map_graph_json(t, "metrics_test", json).await?;
    Ok("metrics_test".to_string())
}

/// Graph with properties, tagged edges, dynamic edges, and a cycle.
///
/// ```text
///   budget_a ──> middle ──> leaf
///   other ──> budget_b ──> leaf
///   route_a ─[tag: lazy]─> hub ─[platform:widget/ios]─> ios_impl ──> leaf
///                              ─[platform:widget/android]─> android_impl ──> leaf
///   cycle_a ──> cycle_b ──> cycle_a (cycle)
///   cycle_a ──> leaf
/// ```
///
/// Properties:
///   budget_a: {type: budget}
///   budget_b: {type: budget}
///
/// Entrypoints (parentless): budget_a, other, route_a, cycle_a
async fn ingest_with_properties(t: &crate::support::app::TestApp) -> Result<String> {
    let json = r#"{
        "nodes": {
            "budget_a": {
                "properties": {"type": "budget"},
                "edges_directed": ["middle"]
            },
            "middle": {
                "edges_directed": ["leaf"]
            },
            "leaf": {},
            "other": {
                "edges_directed": ["budget_b"]
            },
            "budget_b": {
                "properties": {"type": "budget"},
                "edges_directed": ["leaf"]
            },
            "route_a": {
                "edges_tagged": { "lazy": ["hub"] }
            },
            "hub": {
                "edges_dynamic": {
                    "platform": {
                        "widget": {
                            "branches": {
                                "ios": ["ios_impl"],
                                "android": ["android_impl"]
                            }
                        }
                    }
                }
            },
            "ios_impl": {
                "edges_directed": ["leaf"]
            },
            "android_impl": {
                "edges_directed": ["leaf"]
            },
            "cycle_a": {
                "edges_directed": ["cycle_b", "leaf"]
            },
            "cycle_b": {
                "edges_directed": ["cycle_a"]
            }
        }
    }"#;
    crate::support::fixtures::ingest_map_graph_json(t, "props_test", json).await?;
    Ok("props_test".to_string())
}

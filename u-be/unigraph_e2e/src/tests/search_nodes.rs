// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Result;
use k9::snapshot;
use unigraph_app::SearchMode;
use unigraph_app::SearchNodeMatch;
use unigraph_app::SearchNodesInput;
use unigraph_app::call_rpc;
use unigraph_storage_core::TimelineID;

use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn exact_match() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("app".into()),
            limit: None,
            mode: None,
            match_properties: None,
        })
    );
    snapshot!(names(&out.matches), "app");

    Ok(())
}

#[tokio::test]
async fn fuzzy_subsequence() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("co".into()),
            limit: None,
            mode: None,
            match_properties: None,
        })
    );
    // "co" subsequence matches: core, components (shortest first)
    snapshot!(
        names(&out.matches),
        "
core
components
"
    );

    Ok(())
}

#[tokio::test]
async fn limit_results() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("a".into()),
            limit: Some(3),
            mode: None,
            match_properties: None,
        })
    );
    // "a" matches many nodes; we only want the 3 shortest
    assert_eq!(out.matches.len(), 3);

    Ok(())
}

#[tokio::test]
async fn no_matches() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("zzzzz".into()),
            limit: None,
            mode: None,
            match_properties: None,
        })
    );
    assert!(out.matches.is_empty());

    Ok(())
}

#[tokio::test]
async fn exact_match_mode() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid.clone(),
            pattern: Some("app".into()),
            limit: None,
            mode: Some(SearchMode::ExactMatch),
            match_properties: None,
        })
    );
    snapshot!(names(&out.matches), "app");

    // Non-existent exact name returns empty
    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("nonexistent".into()),
            limit: None,
            mode: Some(SearchMode::ExactMatch),
            match_properties: None,
        })
    );
    assert!(out.matches.is_empty());

    Ok(())
}

#[tokio::test]
async fn property_only_search() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: None,
            limit: None,
            mode: None,
            match_properties: Some(BTreeMap::from([(
                "type".to_string(),
                "service".to_string()
            ),])),
        })
    );
    snapshot!(
        names(&out.matches),
        "
auth_service
user_service
"
    );

    Ok(())
}

#[tokio::test]
async fn pattern_with_properties() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("on".into()),
            limit: None,
            mode: Some(SearchMode::Fuzzy),
            match_properties: Some(BTreeMap::from([
                ("type".to_string(), "component".to_string()),
                ("platform".to_string(), "web".to_string()),
            ])),
        })
    );
    snapshot!(names(&out.matches), "button");

    Ok(())
}

#[tokio::test]
async fn no_matches_with_properties() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: None,
            limit: None,
            mode: None,
            match_properties: Some(BTreeMap::from([(
                "type".to_string(),
                "nonexistent".to_string()
            ),])),
        })
    );
    assert!(out.matches.is_empty());

    Ok(())
}

#[tokio::test]
async fn wrong_property_value() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: None,
            limit: None,
            mode: None,
            match_properties: Some(BTreeMap::from(
                [("type".to_string(), "widget".to_string()),]
            )),
        })
    );
    assert!(out.matches.is_empty());

    Ok(())
}

#[tokio::test]
async fn results_include_node_data() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            pattern: Some("button".into()),
            limit: None,
            mode: Some(SearchMode::ExactMatch),
            match_properties: None,
        })
    );
    assert_eq!(out.matches.len(), 1);
    let m = &out.matches[0];
    assert_eq!(m.name, "button");
    snapshot!(
        serde_json::to_string_pretty(&m.node).unwrap(),
        r#"
{
  "properties": {
    "platform": "web",
    "type": "component"
  },
  "edges_directed": [
    "utils"
  ]
}
"#
    );

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

fn names(matches: &[SearchNodeMatch]) -> String {
    matches
        .iter()
        .map(|m| m.name.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Fixtures ─────────────────────────────────────────────────────

async fn ingest_with_search_properties(t: &crate::support::app::TestApp) -> Result<String> {
    let json = r#"{
        "nodes": {
            "button": {
                "properties": {"type": "component", "platform": "web"},
                "edges_directed": ["utils"]
            },
            "dialog": {
                "properties": {"type": "component", "platform": "mobile"},
                "edges_directed": ["utils"]
            },
            "auth_service": {
                "properties": {"type": "service"},
                "edges_directed": ["db"]
            },
            "user_service": {
                "properties": {"type": "service"},
                "edges_directed": ["db"]
            },
            "utils": {},
            "db": {}
        }
    }"#;
    crate::support::fixtures::ingest_map_graph_json(t, "search_props", json).await?;
    Ok("search_props".to_string())
}

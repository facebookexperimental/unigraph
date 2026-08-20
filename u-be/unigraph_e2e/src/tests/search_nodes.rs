// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::SearchNodeMatch;
use unigraph_app::SearchNodesInput;
use unigraph_app::call_rpc;
use unigraph_core::NameMatchMode;
use unigraph_core::NodeSelection;
use unigraph_core::PropertyValueMatch;
use unigraph_storage_core::TimelineID;

use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── Tests ────────────────────────────────────────────────────────

/// The default mode is `Substring`, so a bare pattern matches anywhere in the
/// name rather than as a subsequence.
#[tokio::test]
async fn substring_is_the_default_mode() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            selection: name("app"),
            limit: None,
        })
    );
    snapshot!(names(&out.matches), "app");

    Ok(())
}

/// `Fuzzy` is the typeahead's mode: a subsequence match, shortest name first.
#[tokio::test]
async fn fuzzy_subsequence() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid.clone(),
            selection: name_mode("co", NameMatchMode::Fuzzy),
            limit: None,
        })
    );
    snapshot!(
        names(&out.matches),
        "
core
components
"
    );

    // "cts" is a subsequence of "components" but not a substring of anything,
    // which is what actually separates the two modes.
    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid.clone(),
            selection: name_mode("cts", NameMatchMode::Fuzzy),
            limit: None,
        })
    );
    snapshot!(names(&out.matches), "components");

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            selection: name("cts"),
            limit: None,
        })
    );
    assert!(
        out.matches.is_empty(),
        "substring mode must not match a mere subsequence"
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
            selection: name_mode("a", NameMatchMode::Fuzzy),
            limit: Some(3),
        })
    );
    assert_eq!(out.matches.len(), 3, "the limit should cap the match count");

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
            selection: name("zzzzz"),
            limit: None,
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
            selection: name_mode("app", NameMatchMode::Exact),
            limit: None,
        })
    );
    snapshot!(names(&out.matches), "app");

    // Non-existent exact name returns empty
    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            selection: name_mode("nonexistent", NameMatchMode::Exact),
            limit: None,
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
            selection: prop("type", Some("service")),
            limit: None,
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

/// An absent value means "carries this property at all" — a condition the old
/// `BTreeMap<String, String>` shape couldn't express.
#[tokio::test]
async fn property_presence_search() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            selection: prop("platform", None),
            limit: None,
        })
    );
    snapshot!(
        names(&out.matches),
        "
button
dialog
"
    );

    Ok(())
}

#[tokio::test]
async fn pattern_with_properties() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let mut selection = name_mode("on", NameMatchMode::Fuzzy);
    selection.properties = [
        ("type".to_string(), value("component")),
        ("platform".to_string(), value("web")),
    ]
    .into();

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            selection,
            limit: None,
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
            selection: prop("type", Some("nonexistent")),
            limit: None,
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
            selection: prop("type", Some("widget")),
            limit: None,
        })
    );
    assert!(out.matches.is_empty());

    Ok(())
}

/// An empty selection matches everything — the "just list some nodes" case.
#[tokio::test]
async fn empty_selection_lists_nodes() -> Result<()> {
    let t = init_app();
    let handle = ingest_with_search_properties(&t).await?;
    let tid = TimelineID(handle);

    let out = call_rpc!(
        t,
        SearchNodes(SearchNodesInput {
            timeline_id: tid,
            selection: NodeSelection::default(),
            limit: Some(3),
        })
    );
    snapshot!(
        names(&out.matches),
        "
auth_service
button
db
"
    );

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
            selection: name_mode("button", NameMatchMode::Exact),
            limit: None,
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

fn name(pattern: &str) -> NodeSelection {
    name_mode(pattern, NameMatchMode::Substring)
}

fn name_mode(pattern: &str, mode: NameMatchMode) -> NodeSelection {
    NodeSelection::by_name(pattern, mode)
}

fn prop(key: &str, expected: Option<&str>) -> NodeSelection {
    NodeSelection::by_property(key, expected.map(str::to_string))
}

fn value(expected: &str) -> PropertyValueMatch {
    PropertyValueMatch {
        value: Some(expected.to_string()),
    }
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

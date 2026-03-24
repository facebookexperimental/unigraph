// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
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
            pattern: "app".into(),
            limit: None,
        })
    );
    snapshot!(out.matches.join("\n"), "app");

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
            pattern: "co".into(),
            limit: None,
        })
    );
    // "co" subsequence matches: core, components (shortest first)
    snapshot!(
        out.matches.join("\n"),
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
            pattern: "a".into(),
            limit: Some(3),
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
            pattern: "zzzzz".into(),
            limit: None,
        })
    );
    assert!(out.matches.is_empty());

    Ok(())
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use k9::snapshot;
use unigraph_app::ExploreGraphInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::GraphHandle;
use unigraph_app::MetricView;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::Decision;
use unigraph_core::TraversalConfig;
use unigraph_core::TraversalOverride;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn consistent_across_handle_types() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, bare_gqc(&timeline)?).await?;
    let nested_key = store_gqc(&t, bare_gqc_from_key(gqc_key.clone())).await?;

    let by_timeline = drill_app_ascii(&t, bare_gqc(&timeline)?).await?;
    let by_graph_key = drill_app_ascii(&t, bare_gqc(&format!("{timeline}~0"))?).await?;
    let by_gqc = drill_app_ascii(&t, bare_gqc_from_key(gqc_key)).await?;
    let by_nested = drill_app_ascii(&t, bare_gqc_from_key(nested_key)).await?;

    assert_eq!(by_timeline, by_graph_key, "timeline vs graph_key");
    assert_eq!(by_timeline, by_gqc, "timeline vs gqc_key");
    assert_eq!(by_timeline, by_nested, "timeline vs nested_gqc");

    snapshot!(
        by_timeline,
        "
Edges: forward
Edges of: app

node_name | size | size~transitive ▼
==========+======+==================
app       |  500 |              1985
----------+------+------------------
core      |  300 |               870
ui        |  200 |               665
utils     |   50 |                50

"
    );

    Ok(())
}

#[tokio::test]
async fn root_overrides() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;

    let gqc = GraphQueryConfig {
        handle: timeline.parse()?,
        roots: Some(BTreeSet::from(["ui".to_string()])),
        traversal: None,
    };
    let gqc_key = store_gqc(&t, gqc).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(explore_input(
            bare_gqc_from_key(gqc_key),
            ExploreGraphTarget::EntryPoints {},
        ))
    );

    snapshot!(
        out.ascii.unwrap(),
        "
Entry points

node_name | size | size~transitive ▼
==========+======+==================
ui        |  200 |               665

"
    );

    Ok(())
}

#[tokio::test]
async fn traversal_overrides_inline_and_by_key() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;

    let tvc_key = store_tvc(&t, exclude_utils_tvc()).await?;

    let by_inline = drill_app_ascii(
        &t,
        GraphQueryConfig {
            handle: timeline.parse()?,
            roots: None,
            traversal: Some(TraversalOverride::Inline(exclude_utils_tvc())),
        },
    )
    .await?;

    let by_key = drill_app_ascii(
        &t,
        GraphQueryConfig {
            handle: timeline.parse()?,
            roots: None,
            traversal: Some(TraversalOverride::Key(tvc_key)),
        },
    )
    .await?;

    assert_eq!(by_inline, by_key, "inline TVC vs stored TVC key");

    snapshot!(
        by_inline,
        "
Edges: forward
Edges of: app

node_name | size | size~transitive ▼
==========+======+==================
app       |  500 |              1935
----------+------+------------------
core      |  300 |               820
ui        |  200 |               615

"
    );

    Ok(())
}

#[tokio::test]
async fn nested_gqc_with_root_override() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;

    let base_key = store_gqc(&t, bare_gqc(&timeline)?).await?;

    let outer = GraphQueryConfig {
        handle: GraphHandle::GqcKey(base_key),
        roots: Some(BTreeSet::from(["core".to_string()])),
        traversal: None,
    };
    let outer_key = store_gqc(&t, outer).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(explore_input(
            bare_gqc_from_key(outer_key),
            ExploreGraphTarget::EntryPoints {},
        ))
    );

    snapshot!(
        out.ascii.unwrap(),
        "
Entry points

node_name | size | size~transitive ▼
==========+======+==================
core      |  300 |               870

"
    );

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

fn bare_gqc(handle_str: &str) -> Result<GraphQueryConfig> {
    Ok(GraphQueryConfig {
        handle: handle_str.parse()?,
        roots: None,
        traversal: None,
    })
}

fn bare_gqc_from_key(key: GraphQueryConfigKey) -> GraphQueryConfig {
    GraphQueryConfig {
        handle: GraphHandle::GqcKey(key),
        roots: None,
        traversal: None,
    }
}

async fn store_gqc(t: &TestApp, gqc: GraphQueryConfig) -> Result<GraphQueryConfigKey> {
    let put = call_rpc!(
        t,
        PutConfigs(PutConfigsInput {
            traversal_configs: vec![],
            graph_query_configs: vec![gqc],
        })
    );
    Ok(put.graph_query_configs.into_iter().next().unwrap())
}

async fn store_tvc(t: &TestApp, tvc: TraversalConfig) -> Result<TraversalConfigKey> {
    let put = call_rpc!(
        t,
        PutConfigs(PutConfigsInput {
            traversal_configs: vec![tvc],
            graph_query_configs: vec![],
        })
    );
    Ok(put.traversal_configs.into_iter().next().unwrap())
}

fn parse_metrics(strs: &[&str]) -> Vec<MetricView> {
    strs.iter()
        .map(|s| {
            s.parse()
                .unwrap_or_else(|e| panic!("bad metric view '{s}': {e}"))
        })
        .collect()
}

fn explore_input(query: GraphQueryConfig, target: ExploreGraphTarget) -> ExploreGraphInput {
    ExploreGraphInput {
        query,
        target,
        graph_structure: GraphStructure::Forward,
        metrics: Some(parse_metrics(&["size", "size~transitive"])),
        sort_by: Some("size~transitive".parse().unwrap()),
        sort_order: None,
        offset: None,
        limit: None,
        include_ascii: Some(true),
    }
}

async fn drill_app_ascii(t: &TestApp, query: GraphQueryConfig) -> Result<String> {
    let out = call_rpc!(
        t,
        ExploreGraph(explore_input(
            query,
            ExploreGraphTarget::Node {
                name: "app".to_string(),
            },
        ))
    );
    Ok(out.ascii.unwrap())
}

// ── Fixtures ────────────────────────────────────────────────────

fn exclude_utils_tvc() -> TraversalConfig {
    let mut force_nodes = BTreeMap::new();
    force_nodes.insert("utils".to_string(), Decision::exclude());
    TraversalConfig {
        force_nodes: Some(force_nodes),
        force_edges: None,
        force_tagged: None,
        label_predicates: None,
        force_dynamic: None,
        tiered_traversal: None,
        messages: None,
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Dominated sizes are relative to the reachable subgraph, so lowering
//! `max_tier` can make a node's dominated size *grow*: excluding the edges that
//! transition above the cap removes bypass paths, and nodes that used to be
//! reachable around a module now funnel through it.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::ExploreGraphInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::GraphHandle;
use unigraph_app::MetricView;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::MapGraph;
use unigraph_core::TieredTraversalConfig;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_map_graph_json;

// ── Tests ────────────────────────────────────────────────────────

/// ```text
/// root ─▶ a ─rdfd─▶ x ─▶ y
///   └───▶ b ──rd──────────┘
/// ```
///
/// Uncapped, `y` is reachable via `b` (rd → t3) as well as via `x`, so `x`
/// dominates only itself. `y` still lands in t2 because tiers take the minimum
/// over paths.
#[tokio::test]
async fn dominated_uncapped() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest_graph(&t, "dominated_uncapped", None).await?;

    let out = call_rpc!(t, ExploreGraph(explore_all(gqc_key)));
    snapshot!(
        out.ascii.unwrap(),
        "
All reachable nodes

node_name | size ▼ | size#t2~dominated | size~dominated | tier
==========+========+===================+================+=====
y         |    100 |               100 |            100 |   t2
x         |     10 |                10 |             10 |   t2
b         |      4 |                 4 |              4 |   t1
a         |      2 |                12 |             12 |   t1
root      |      1 |               117 |            117 |   t1

"
    );

    Ok(())
}

/// With `max_tier = 1` (t2) the `rd` edge is excluded, so the only path to `y`
/// is through `x` and `size#t2~dominated` of `x` grows from 10 to 110.
#[tokio::test]
async fn dominated_capped_at_t2_grows() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest_graph(&t, "dominated_capped", Some(1)).await?;

    let out = call_rpc!(t, ExploreGraph(explore_all(gqc_key)));
    snapshot!(
        out.ascii.unwrap(),
        "
All reachable nodes

node_name | size ▼ | size#t2~dominated | size~dominated | tier
==========+========+===================+================+=====
y         |    100 |               100 |            100 |   t2
x         |     10 |               110 |            110 |   t2
b         |      4 |                 4 |              4 |   t1
a         |      2 |               112 |            112 |   t1
root      |      1 |               117 |            117 |   t1

"
    );

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

async fn ingest_graph(
    t: &TestApp,
    timeline_id: &str,
    max_tier: Option<usize>,
) -> Result<GraphQueryConfigKey> {
    let mut map_graph = MapGraph::from_json(GRAPH_JSON)?;
    if let Some(TieredTraversalConfig::AscendingTiers(config)) = map_graph
        .traversal_config
        .as_mut()
        .and_then(|c| c.tiered_traversal.as_mut())
    {
        config.max_tier = max_tier;
    }
    ingest_map_graph_json(t, timeline_id, &map_graph.to_json()?).await?;

    let put = call_rpc!(
        t,
        PutConfigs(PutConfigsInput {
            traversal_configs: vec![],
            graph_query_configs: vec![GraphQueryConfig {
                handle: timeline_id.parse().unwrap(),
                roots: None,
                traversal: None,
            }],
        })
    );
    Ok(put.graph_query_configs.into_iter().next().unwrap())
}

fn explore_all(gqc_key: GraphQueryConfigKey) -> ExploreGraphInput {
    let metric = |s: &str| s.parse::<MetricView>().unwrap();
    ExploreGraphInput {
        query: GraphQueryConfig {
            handle: GraphHandle::GqcKey(gqc_key),
            roots: None,
            traversal: None,
        },
        target: ExploreGraphTarget::AllNodes {},
        graph_structure: GraphStructure::Forward,
        metrics: Some(vec![
            metric("size"),
            metric("tier"),
            metric("size#t2~dominated"),
            metric("size~dominated"),
        ]),
        sort_by: Some(metric("size")),
        sort_order: None,
        offset: None,
        limit: None,
        include_excluded: None,
        include_ascii: Some(true),
    }
}

// ── Fixture ──────────────────────────────────────────────────────

const GRAPH_JSON: &str = r#"{
  "nodes": {
    "root": { "metrics": { "size": 1 }, "edges_directed": ["a", "b"] },
    "a": { "metrics": { "size": 2 }, "edges_tagged": { "rdfd": ["x"] } },
    "b": { "metrics": { "size": 4 }, "edges_tagged": { "rd": ["y"] } },
    "x": { "metrics": { "size": 10 }, "edges_directed": ["y"] },
    "y": { "metrics": { "size": 100 } }
  },
  "traversal_config": {
    "tiered_traversal": {
      "AscendingTiers": {
        "tiers": [
          { "name": "t1", "tags_that_transition_to_this_tier": [] },
          { "name": "t2", "tags_that_transition_to_this_tier": ["rdfd"] },
          { "name": "t3", "tags_that_transition_to_this_tier": ["rd"] }
        ],
        "max_tier": null
      }
    }
  },
  "entry_points": ["root"]
}"#;

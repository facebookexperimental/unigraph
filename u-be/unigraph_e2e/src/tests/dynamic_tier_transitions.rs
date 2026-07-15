// Copyright (c) Meta Platforms, Inc. and affiliates.

//! E2e coverage for dynamic-edge tier transitions.
//!
//! A dynamic edge whose `DynamicTypeKey` (e.g. `"rc:gk"`) is listed in a tier's
//! `dynamic_type_keys_that_transition_to_this_tier` bumps its target node — and
//! everything downstream of it — to that tier, exactly mirroring how a tagged
//! edge transitions via `tags_that_transition_to_this_tier`.

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

/// `entry` fans out three ways: a plain directed edge (stays `eager`), a tagged
/// `lazy` edge (transitions to `lazy`), and a dynamic `rc:gk` edge (transitions
/// to `gk_gated`). The dynamic transition propagates downstream: `gk_grandchild`
/// inherits `gk_gated` even though it's reached by a plain directed edge, because
/// tiers only ascend.
#[tokio::test]
async fn dynamic_type_key_transitions_to_tier() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest_gk_graph(&t, "gk_transitions", None).await?;

    let out = call_rpc!(t, ExploreGraph(explore_all_tiers(gqc_key)));
    snapshot!(
        out.ascii.unwrap(),
        "
All reachable nodes

node_name     | size~transitive |   tier ▼
==============+=================+=========
gk_grandchild |              50 | gk_gated
gk_off        |              40 | gk_gated
gk_on         |              80 | gk_gated
lazy_mod      |              20 |     lazy
entry         |             250 |    eager
normal        |              10 |    eager

"
    );

    Ok(())
}

/// With `max_tier = 1`, the `gk_gated` tier (index 2) is above the max, so the
/// dynamic `rc:gk` edges are excluded outright and everything they reach becomes
/// unreachable — only the `eager`/`lazy` subgraph survives. This mirrors the
/// max-tier exclusion that already applied to tagged edges.
#[tokio::test]
async fn max_tier_excludes_dynamic_transition() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest_gk_graph(&t, "gk_max_tier", Some(1)).await?;

    let out = call_rpc!(t, ExploreGraph(explore_all_tiers(gqc_key)));
    snapshot!(
        out.ascii.unwrap(),
        "
All reachable nodes

node_name | size~transitive | tier ▼
==========+=================+=======
lazy_mod  |              20 |   lazy
entry     |             130 |  eager
normal    |              10 |  eager

"
    );

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

/// Parse the base fixture, override `max_tier`, ingest it, and return a GQC key.
async fn ingest_gk_graph(
    t: &TestApp,
    timeline_id: &str,
    max_tier: Option<usize>,
) -> Result<GraphQueryConfigKey> {
    let mut map_graph = MapGraph::from_json(GK_GRAPH_JSON)?;
    set_max_tier(&mut map_graph, max_tier);
    ingest_map_graph_json(t, timeline_id, &map_graph.to_json()?).await?;
    store_gqc(t, timeline_id).await
}

fn set_max_tier(map_graph: &mut MapGraph, max_tier: Option<usize>) {
    if let Some(TieredTraversalConfig::AscendingTiers(config)) = map_graph
        .traversal_config
        .as_mut()
        .and_then(|c| c.tiered_traversal.as_mut())
    {
        config.max_tier = max_tier;
    }
}

async fn store_gqc(t: &TestApp, handle: &str) -> Result<GraphQueryConfigKey> {
    let gqc = GraphQueryConfig {
        handle: handle.parse().unwrap(),
        roots: None,
        traversal: None,
    };
    let put = call_rpc!(
        t,
        PutConfigs(PutConfigsInput {
            traversal_configs: vec![],
            graph_query_configs: vec![gqc],
        })
    );
    Ok(put.graph_query_configs.into_iter().next().unwrap())
}

fn explore_all_tiers(gqc_key: GraphQueryConfigKey) -> ExploreGraphInput {
    ExploreGraphInput {
        query: GraphQueryConfig {
            handle: GraphHandle::GqcKey(gqc_key),
            roots: None,
            traversal: None,
        },
        target: ExploreGraphTarget::AllNodes {},
        graph_structure: GraphStructure::Forward,
        metrics: Some(vec![
            "size~transitive".parse::<MetricView>().unwrap(),
            "tier".parse::<MetricView>().unwrap(),
        ]),
        sort_by: Some("tier".parse::<MetricView>().unwrap()),
        sort_order: None,
        offset: None,
        limit: None,
        include_ascii: Some(true),
    }
}

// ── Fixture ──────────────────────────────────────────────────────

const GK_GRAPH_JSON: &str = r#"{
  "nodes": {
    "entry": {
      "metrics": { "size": 100 },
      "edges_directed": ["normal"],
      "edges_tagged": { "lazy": ["lazy_mod"] },
      "edges_dynamic": {
        "rc:gk": {
          "my_gk_check": {
            "branches": {
              "true": ["gk_on"],
              "false": ["gk_off"]
            }
          }
        }
      }
    },
    "normal": { "metrics": { "size": 10 } },
    "lazy_mod": { "metrics": { "size": 20 } },
    "gk_on": { "metrics": { "size": 30 }, "edges_directed": ["gk_grandchild"] },
    "gk_off": { "metrics": { "size": 40 } },
    "gk_grandchild": { "metrics": { "size": 50 } }
  },
  "traversal_config": {
    "tiered_traversal": {
      "AscendingTiers": {
        "tiers": [
          { "name": "eager", "tags_that_transition_to_this_tier": [] },
          { "name": "lazy", "tags_that_transition_to_this_tier": ["lazy"] },
          {
            "name": "gk_gated",
            "tags_that_transition_to_this_tier": [],
            "dynamic_type_keys_that_transition_to_this_tier": ["rc:gk"]
          }
        ],
        "max_tier": null
      }
    }
  },
  "entry_points": ["entry"]
}"#;

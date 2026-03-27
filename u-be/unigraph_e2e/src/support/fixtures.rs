// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Shared fixture graphs for e2e tests.
//!
//! Each fixture function ingests a graph into the test app's DB and returns
//! the timeline ID, ready for use in RPC calls.

use anyhow::Result;
use unigraph_core::GraphID;
use unigraph_core::GraphTimeKey;
use unigraph_core::MapGraph;
use unigraph_core::Timestamp;
use unigraph_storage_core::AdjacentDeltasConfig;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;

use super::app::TestApp;

/// Ingest the comprehensive "explore_graph" fixture into the test app.
/// Returns the timeline ID for use in RPC handles.
pub async fn ingest_explore_graph(t: &TestApp) -> Result<String> {
    let json = include_str!("fixtures/explore_graph.json");
    let timeline_id = "explore_test";
    ingest_map_graph_json(t, timeline_id, json).await?;
    Ok(timeline_id.to_string())
}

/// Ingest a MapGraph JSON string into the test app under the given timeline ID.
async fn ingest_map_graph_json(t: &TestApp, timeline_id: &str, json: &str) -> Result<()> {
    let map_graph = MapGraph::from_json(json)?;
    let ag_ser = map_graph.to_array_graph_serializable()?;
    let tid = TimelineID(timeline_id.to_string());

    t.app
        .db
        .timelines
        .create(&tid, &default_timeline_config(), &t.task)
        .await?;

    let key = GraphTimeKey {
        timeline_id: tid,
        timestamp: Timestamp::from_unix_timestamp(1000),
        graph_id: GraphID(0),
    };
    t.app.db.graph.store(&key, &ag_ser, None, &t.task).await?;

    Ok(())
}

fn default_timeline_config() -> TimelineConfig {
    TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    }
}

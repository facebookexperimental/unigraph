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

/// Ingest the "after" counterpart of the `explore_graph` fixture — the same
/// graph with one of every kind of change, for delta tests:
///
/// | change          | edit                                              |
/// |-----------------|---------------------------------------------------|
/// | metrics changed | `core.size` 300 → 420                             |
/// | node added      | `telemetry`, reachable via `core`                 |
/// | node removed    | `analytics`, with `core`'s `lazy` edge to it      |
/// | edges changed   | `ui` gains a directed edge to `utils`             |
/// | dynamic edge    | `components.platform.button` gains a `web` branch |
///
/// Everything else (`db`, `auth`, `styles`, `dialogs`, `button_ios`,
/// `button_android`) is untouched — that untouched region is what
/// `changed_nodes_only` collapses.
pub async fn ingest_explore_graph_after(t: &TestApp) -> Result<String> {
    let json = include_str!("fixtures/explore_graph_after.json");
    let timeline_id = "explore_test_after";
    ingest_map_graph_json(t, timeline_id, json).await?;
    Ok(timeline_id.to_string())
}

/// Ingest the "before" side of the minimal delta-semantics fixture — a plain
/// chain, sized so every number in a delta snapshot is checkable by hand:
///
/// ```text
///   root(100) → shared_a(40) → shared_b(20) → shared_c(10)
/// ```
///
/// Pairs with [`ingest_delta_semantics_after`]. Deliberately carries no
/// `metrics_config`, so `size` renders as a raw number rather than a formatted
/// size — the point of these tests is the arithmetic, not the formatting.
pub async fn ingest_delta_semantics(t: &TestApp) -> Result<String> {
    let json = include_str!("fixtures/delta_semantics.json");
    let timeline_id = "delta_semantics";
    ingest_map_graph_json(t, timeline_id, json).await?;
    Ok(timeline_id.to_string())
}

/// Ingest the "after" side of the delta-semantics fixture: one node is added,
/// and it depends on a node that already existed.
///
/// ```text
///   root(100) → shared_a(40) → shared_b(20) → shared_c(10)
///        └────→ newcomer(5) ──────┘
/// ```
///
/// This is the worked example from `twin_graph/metrics.rs` made concrete, and
/// it is what makes the two delta semantics disagree:
///
/// - `newcomer` pulls in a 3-node subtree but only *adds* itself, so its
///   exclusive count delta (`+1`) differs from a plain `R - L` (`+3`).
/// - `shared_b` gains a second parent, so it stops being dominated by
///   `shared_a` — `shared_a`'s dominated subtree shrinks even though
///   `shared_a` itself is byte-for-byte unchanged.
pub async fn ingest_delta_semantics_after(t: &TestApp) -> Result<String> {
    let json = include_str!("fixtures/delta_semantics_after.json");
    let timeline_id = "delta_semantics_after";
    ingest_map_graph_json(t, timeline_id, json).await?;
    Ok(timeline_id.to_string())
}

/// Ingest the "explore_graph_two_entry_points" fixture into the test app.
///
/// This is the same graph as `ingest_explore_graph`, but with an extra
/// standalone `root` node and no explicit `entry_points`, so the system
/// auto-detects entry points (nodes with no parents) — yielding two:
/// `app` and `root`.
/// Returns the timeline ID for use in RPC handles.
pub async fn ingest_two_entry_points_graph(t: &TestApp) -> Result<String> {
    let json = include_str!("fixtures/explore_graph_two_entry_points.json");
    let timeline_id = "explore_two_entry_points";
    ingest_map_graph_json(t, timeline_id, json).await?;
    Ok(timeline_id.to_string())
}

/// Ingest a MapGraph JSON string into the test app under the given timeline ID.
pub async fn ingest_map_graph_json(t: &TestApp, timeline_id: &str, json: &str) -> Result<()> {
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

/// The timeline config every fixture uses: `AdjacentDeltas`, inline blobs,
/// no external ID namespace, no metric history.
pub fn default_timeline_config() -> TimelineConfig {
    TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    }
}

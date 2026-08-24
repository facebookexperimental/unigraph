// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::FrameInfo;
use unigraph_app::SelectFramesInput;
use unigraph_app::call_rpc;
use unigraph_core::GraphID;
use unigraph_core::GraphTimeKey;
use unigraph_core::MapGraph;
use unigraph_core::Timestamp;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimestampedError;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::default_timeline_config;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn lists_frames_without_error_content_by_default() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_mixed_timeline(&t).await?;

    let out = call_rpc!(t, SelectFrames(select_input(&timeline_id)));

    snapshot!(
        format_frames(&out.frames),
        "
graph_id  timestamp                  type    errors
0         1970-01-01T00:16:40+00:00  Full    -
1         1970-01-01T00:33:20+00:00  Error   -
2         1970-01-01T00:50:00+00:00  Full    -
3         1970-01-01T01:06:40+00:00  Error   -
"
    );

    Ok(())
}

#[tokio::test]
async fn includes_error_content_when_requested() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_mixed_timeline(&t).await?;

    let out = call_rpc!(
        t,
        SelectFrames(SelectFramesInput {
            include_error_info: Some(true),
            ..select_input(&timeline_id)
        })
    );

    snapshot!(
        format_frames(&out.frames),
        "
graph_id  timestamp                  type    errors
0         1970-01-01T00:16:40+00:00  Full    -
1         1970-01-01T00:33:20+00:00  Error   2: [1970-01-01T00:33:20+00:00] build failed: missing target | [1970-01-01T00:33:21+00:00] retry failed: missing target
2         1970-01-01T00:50:00+00:00  Full    -
3         1970-01-01T01:06:40+00:00  Error   1: [1970-01-01T01:06:40+00:00] parse failed: unexpected token
"
    );

    Ok(())
}

#[tokio::test]
async fn graph_ids_select_exactly_the_named_frames() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_mixed_timeline(&t).await?;

    // 7 does not exist: asking for a missing graph_id is how callers test for
    // presence, so it must return the found subset rather than erroring.
    let out = call_rpc!(
        t,
        SelectFrames(SelectFramesInput {
            graph_ids: Some(vec![2, 0, 7]),
            ..select_input(&timeline_id)
        })
    );

    snapshot!(
        format_frames(&out.frames),
        "
graph_id  timestamp                  type    errors
0         1970-01-01T00:16:40+00:00  Full    -
2         1970-01-01T00:50:00+00:00  Full    -
"
    );

    Ok(())
}

#[tokio::test]
async fn graph_ids_compose_with_the_other_filters() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_mixed_timeline(&t).await?;

    let out = call_rpc!(
        t,
        SelectFrames(SelectFramesInput {
            graph_ids: Some(vec![1, 2, 3]),
            frame_types: Some(vec!["Error".to_owned()]),
            order: Some("Desc".to_owned()),
            ..select_input(&timeline_id)
        })
    );

    snapshot!(
        format_frames(&out.frames),
        "
graph_id  timestamp                  type    errors
3         1970-01-01T01:06:40+00:00  Error   -
1         1970-01-01T00:33:20+00:00  Error   -
"
    );

    Ok(())
}

#[tokio::test]
async fn no_matching_graph_ids_yields_no_frames() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_mixed_timeline(&t).await?;

    let out = call_rpc!(
        t,
        SelectFrames(SelectFramesInput {
            graph_ids: Some(vec![404]),
            ..select_input(&timeline_id)
        })
    );

    assert!(
        out.frames.is_empty(),
        "An unmatched graph_id must return nothing, not fall back to the whole timeline"
    );

    Ok(())
}

#[tokio::test]
async fn error_content_composes_with_filters() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_mixed_timeline(&t).await?;

    let out = call_rpc!(
        t,
        SelectFrames(SelectFramesInput {
            frame_types: Some(vec!["Error".to_owned()]),
            order: Some("Desc".to_owned()),
            limit: Some(1),
            include_error_info: Some(true),
            ..select_input(&timeline_id)
        })
    );

    snapshot!(
        format_frames(&out.frames),
        "
graph_id  timestamp                  type    errors
3         1970-01-01T01:06:40+00:00  Error   1: [1970-01-01T01:06:40+00:00] parse failed: unexpected token
"
    );

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

fn select_input(timeline_id: &str) -> SelectFramesInput {
    SelectFramesInput {
        timeline_id: TimelineID(timeline_id.to_owned()),
        limit: None,
        frame_types: None,
        order: None,
        timestamp_start: None,
        timestamp_end: None,
        graph_ids: None,
        include_error_info: None,
    }
}

/// One row per frame, with the error payload flattened to
/// `<count>: [<ts>] <msg> | [<ts>] <msg>` (or `-` when absent).
fn format_frames(frames: &[FrameInfo]) -> String {
    let header = format!(
        "{:<9} {:<26} {:<7} {}",
        "graph_id", "timestamp", "type", "errors"
    );
    let rows = frames.iter().map(|f| {
        format!(
            "{:<9} {:<26} {:<7} {}",
            f.graph_id,
            f.timestamp,
            f.frame_type,
            format_error(f)
        )
    });
    std::iter::once(header)
        .chain(rows)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_error(frame: &FrameInfo) -> String {
    let Some(error) = &frame.error else {
        return "-".to_owned();
    };
    let messages = error
        .errors
        .iter()
        .map(|e| format!("[{}] {}", e.timestamp, e.message))
        .collect::<Vec<_>>()
        .join(" | ");
    format!("{}: {}", error.error_count, messages)
}

// ── Fixtures ─────────────────────────────────────────────────────

/// A timeline of four frames alternating `Full`, `Error`, `Full`, `Error`.
async fn ingest_mixed_timeline(t: &TestApp) -> Result<String> {
    let timeline_id = "select_frames_test";
    let tid = TimelineID(timeline_id.to_owned());
    t.app
        .db
        .timelines
        .create(&tid, &default_timeline_config(), &t.task)
        .await?;

    store_graph(t, &tid, 0, 1000).await?;
    store_errors(
        t,
        &tid,
        1,
        2000,
        &[
            (2000, "build failed: missing target"),
            (2001, "retry failed: missing target"),
        ],
    )
    .await?;
    store_graph(t, &tid, 2, 3000).await?;
    store_errors(
        t,
        &tid,
        3,
        4000,
        &[(4000, "parse failed: unexpected token")],
    )
    .await?;

    Ok(timeline_id.to_owned())
}

async fn store_graph(t: &TestApp, tid: &TimelineID, graph_id: i64, unix_ts: i64) -> Result<()> {
    let json = r#"{ "nodes": { "a": { "metrics": { "size": 1 } } } }"#;
    let graph = MapGraph::from_json(json)?.to_array_graph_serializable()?;
    t.app
        .db
        .graph
        .store(&key(tid, graph_id, unix_ts), &graph, None, &t.task)
        .await
}

async fn store_errors(
    t: &TestApp,
    tid: &TimelineID,
    graph_id: i64,
    unix_ts: i64,
    errors: &[(i64, &str)],
) -> Result<()> {
    let errors: Vec<TimestampedError> = errors
        .iter()
        .map(|(ts, message)| TimestampedError {
            timestamp: Timestamp::from_unix_timestamp(*ts),
            message: (*message).to_owned(),
        })
        .collect();
    t.app
        .db
        .graph
        .store_error(&key(tid, graph_id, unix_ts), &errors, &t.task)
        .await?;
    Ok(())
}

fn key(tid: &TimelineID, graph_id: i64, unix_ts: i64) -> GraphTimeKey {
    GraphTimeKey {
        timeline_id: tid.clone(),
        timestamp: Timestamp::from_unix_timestamp(unix_ts),
        graph_id: GraphID(graph_id),
    }
}

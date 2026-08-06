// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::LazyLock;

use anyhow::Result;
use k9::snapshot;
use unigraph_app::GetHistoryInput;
use unigraph_app::GetHistoryOutput;
use unigraph_app::UnigraphRequest;
use unigraph_app::call_rpc;
use unigraph_core::GraphID;
use unigraph_core::GraphTimeKey;
use unigraph_core::MapGraph;
use unigraph_core::Timestamp;
use unigraph_db::HistoryIngestOptions;
use unigraph_storage_core::TimelineID;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::default_timeline_config;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn returns_columnar_history_for_multiple_nodes() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_history(&t).await?;

    let out = call_rpc!(
        t,
        GetHistory(GetHistoryInput {
            node_names: vec!["app".to_owned(), "util".to_owned()],
            ..history_input(&timeline_id)
        })
    );

    snapshot!(
        format_history(&out),
        "
metrics: lines, size
frames:  #0 graph_id=0 t+0s | #1 graph_id=1 t+10s | #2 graph_id=2 t+20s

app
  #0  lines=10  size=100
  #1  lines=20  size=300
  #2  lines=20  size=900
util
  #0  lines=1  size=5
  #1  lines=-  size=-
  #2  lines=4  size=50
"
    );

    Ok(())
}

#[tokio::test]
async fn frame_table_is_shared_across_nodes() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_history(&t).await?;

    let out = call_rpc!(
        t,
        GetHistory(GetHistoryInput {
            node_names: vec!["app".to_owned(), "util".to_owned()],
            ..history_input(&timeline_id)
        })
    );

    let sample_count: usize = out.series.iter().map(|s| s.samples.len()).sum();
    assert_eq!(
        sample_count, 6,
        "Both nodes have a sample at all 3 frames — util's middle one is the \
         all-null marker recording that it vanished from that frame"
    );
    assert_eq!(
        out.frames.len(),
        3,
        "The 6 samples span only 3 distinct frames, which must be sent once each"
    );
    assert!(
        out.frames
            .windows(2)
            .all(|w| w[0].timestamp <= w[1].timestamp),
        "Frame table must be sorted by timestamp"
    );

    Ok(())
}

#[tokio::test]
async fn time_bounds_narrow_the_result() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_history(&t).await?;

    let out = call_rpc!(
        t,
        GetHistory(GetHistoryInput {
            node_names: vec!["app".to_owned(), "util".to_owned()],
            timestamp_start: Some(offset_timestamp(10).to_rfc3339()),
            timestamp_end: Some(offset_timestamp(10).to_rfc3339()),
            ..history_input(&timeline_id)
        })
    );

    snapshot!(
        format_history(&out),
        "
metrics: lines, size
frames:  #0 graph_id=1 t+0s

app
  #0  lines=20  size=300
util
  #0  lines=-  size=-
"
    );

    Ok(())
}

#[tokio::test]
async fn unknown_node_yields_an_empty_series() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_history(&t).await?;

    let out = call_rpc!(
        t,
        GetHistory(GetHistoryInput {
            node_names: vec!["nope".to_owned()],
            ..history_input(&timeline_id)
        })
    );

    snapshot!(
        format_history(&out),
        "
metrics: (none)
frames:  (none)

nope
  (no samples)
"
    );

    Ok(())
}

#[tokio::test]
async fn empty_node_list_is_rejected() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_history(&t).await?;

    let result = t
        .rpc(UnigraphRequest::GetHistory(GetHistoryInput {
            node_names: vec![],
            ..history_input(&timeline_id)
        }))
        .await;

    let Err(err) = result else {
        panic!("an unscoped history read should be refused, not answered");
    };
    let message = format!("{err:#}");
    assert!(
        message.contains("node_names must not be empty"),
        "Error should say why the request was refused, got: {message}"
    );

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

fn history_input(timeline_id: &str) -> GetHistoryInput {
    GetHistoryInput {
        timeline_id: TimelineID(timeline_id.to_owned()),
        node_names: vec![],
        timestamp_start: None,
        timestamp_end: None,
    }
}

/// Renders the columnar output back into something readable, with absolute
/// timestamps normalized to an offset from the first frame so the snapshot
/// doesn't depend on the wall clock the fixture ingested against.
fn format_history(out: &GetHistoryOutput) -> String {
    let metrics = match out.metrics.is_empty() {
        true => "(none)".to_owned(),
        false => out.metrics.join(", "),
    };
    let mut lines = vec![
        format!("metrics: {metrics}"),
        format!("frames:  {}", format_frames(out)),
        String::new(),
    ];

    for node in &out.series {
        lines.push(node.node_name.clone());
        if node.samples.is_empty() {
            lines.push("  (no samples)".to_owned());
            continue;
        }
        for sample in &node.samples {
            let values = out
                .metrics
                .iter()
                .zip(&sample.values)
                .map(|(name, value)| match value {
                    Some(v) => format!("{name}={v}"),
                    None => format!("{name}=-"),
                })
                .collect::<Vec<_>>()
                .join("  ");
            lines.push(format!("  #{}  {}", sample.frame, values));
        }
    }

    lines.join("\n")
}

fn format_frames(out: &GetHistoryOutput) -> String {
    let Some(first) = out.frames.first() else {
        return "(none)".to_owned();
    };
    let base = Timestamp::from_rfc3339(&first.timestamp).expect("RPC emits RFC3339 timestamps");

    out.frames
        .iter()
        .enumerate()
        .map(|(idx, frame)| {
            let ts = Timestamp::from_rfc3339(&frame.timestamp).expect("RPC emits RFC3339");
            let offset = ts.to_unix_timestamp() - base.to_unix_timestamp();
            format!("#{idx} graph_id={} t+{offset}s", frame.graph_id)
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

// ── Fixtures ─────────────────────────────────────────────────────

/// Wall clock read once, so every frame in a run agrees on "now".
static TEST_NOW: LazyLock<i64> = LazyLock::new(|| Timestamp::now().to_unix_timestamp());

/// A timestamp inside the ingest lookback window.
fn offset_timestamp(offset_secs: i64) -> Timestamp {
    Timestamp::from_unix_timestamp(*TEST_NOW - 100 + offset_secs)
}

/// Three frames of two nodes, then a history ingest pass.
///
/// `util` is missing from the middle frame, which history records as an
/// explicit all-null sample rather than a gap.
async fn ingest_history(t: &TestApp) -> Result<String> {
    let timeline_id = "history_test";
    let tid = TimelineID(timeline_id.to_owned());
    t.app
        .db
        .timelines
        .create(&tid, &default_timeline_config(), &t.task)
        .await?;

    store_frame(t, &tid, 0, 0, &[("app", 10.0, 100.0), ("util", 1.0, 5.0)]).await?;
    store_frame(t, &tid, 1, 10, &[("app", 20.0, 300.0)]).await?;
    store_frame(t, &tid, 2, 20, &[("app", 20.0, 900.0), ("util", 4.0, 50.0)]).await?;

    t.app
        .db
        .graph_history
        .ingest(
            &tid,
            &HistoryIngestOptions {
                lookback_hours: 1,
                settle_hours: 0,
                threshold: 1.0,
                graph_id_bounds: (None, None),
            },
            &t.task,
        )
        .await?;

    Ok(timeline_id.to_owned())
}

async fn store_frame(
    t: &TestApp,
    tid: &TimelineID,
    graph_id: i64,
    offset_secs: i64,
    nodes: &[(&str, f64, f64)],
) -> Result<()> {
    let key = GraphTimeKey {
        timeline_id: tid.clone(),
        timestamp: offset_timestamp(offset_secs),
        graph_id: GraphID(graph_id),
    };
    let graph = MapGraph::from_json(&metric_graph_json(nodes))?.to_array_graph_serializable()?;
    t.app.db.graph.store(&key, &graph, None, &t.task).await
}

/// A graph of disconnected nodes, each carrying a `lines` and a `size` metric.
fn metric_graph_json(nodes: &[(&str, f64, f64)]) -> String {
    let nodes = nodes
        .iter()
        .map(|(name, lines, size)| {
            format!(r#""{name}": {{ "metrics": {{ "lines": {lines}, "size": {size} }} }}"#)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(r#"{{ "nodes": {{ {nodes} }} }}"#)
}

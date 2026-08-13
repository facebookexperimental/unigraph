// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::LazyLock;

use anyhow::Result;
use k9::snapshot;
use unigraph_app::DecodedSample;
use unigraph_app::GetHistoryInput;
use unigraph_app::GetHistoryOutput;
use unigraph_app::SAMPLE_HEADER_LEN;
use unigraph_app::UnigraphRequest;
use unigraph_app::call_rpc;
use unigraph_app::decode_series;
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
async fn returns_decodable_history_for_multiple_nodes() -> Result<()> {
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
        format_history(&out)?,
        "
metrics: lines, size

app
  t+0s   g0  FIRST|ANCHOR           lines=10  size=100  (unattributable)
  t+10s  g1  OVER_THRESHOLD|ANCHOR  lines=20  size=300
  t+20s  g2  OVER_THRESHOLD|LATEST  lines=20  size=900
util
  t+0s   g0  FIRST|ANCHOR           lines=1  size=5  (unattributable)
  t+10s  g1  OVER_THRESHOLD         lines=-  size=-
  t+20s  g2  FIRST|LATEST           lines=4  size=50
"
    );

    Ok(())
}

/// The wire shape itself, since it is what the frontend has to parse and the
/// whole reason this RPC has a custom format.
#[tokio::test]
async fn samples_go_over_the_wire_as_deltas() -> Result<()> {
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
        format_wire(&out),
        "
stride 6 = 4 header + 2 metrics

app   T0, 0, 5, 0, 10, 100  |  10, 1, 1, 1, 10, 200  |  10, 1, 4, 0, 0, 600
util  T0, 0, 5, 0, 1, 5  |  10, 1, -3, 1, null, null  |  10, 1, 7, 0, 3, 45
"
    );

    Ok(())
}

/// The attribution case the anchor exists for, end to end.
///
/// The threshold folds frame 1 away, so on its own the sample at frame 2 reads
/// as +50 — everything that drifted since frame 0. The anchor puts frame 1 back
/// on the wire, and the step actually attributable to frame 2's graph is +5.
#[tokio::test]
async fn a_kept_sample_arrives_with_the_frame_before_it() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_sparse_history(&t).await?;

    let out = call_rpc!(
        t,
        GetHistory(GetHistoryInput {
            node_names: vec!["app".to_owned()],
            ..history_input(&timeline_id)
        })
    );

    snapshot!(
        format_history(&out)?,
        "
metrics: lines, size

app
  t+0s   g0  FIRST                  lines=10  size=100  (unattributable)
  t+10s  g1  ANCHOR                 lines=10  size=145
  t+20s  g2  OVER_THRESHOLD|LATEST  lines=10  size=250
"
    );

    Ok(())
}

#[tokio::test]
async fn every_node_carries_its_own_frame_coordinates() -> Result<()> {
    let t = init_app();
    let timeline_id = ingest_history(&t).await?;

    let out = call_rpc!(
        t,
        GetHistory(GetHistoryInput {
            node_names: vec!["app".to_owned(), "util".to_owned()],
            ..history_input(&timeline_id)
        })
    );

    let stride = SAMPLE_HEADER_LEN + out.metrics.len();
    for node in &out.series {
        assert_eq!(
            node.deltas.len(),
            stride * 3,
            "'{}' has a sample at all 3 frames — util's middle one is the \
             all-null marker recording that it vanished from that frame",
            node.node_name
        );

        let samples = decode_series(&out.metrics, node)?;
        assert!(
            samples.windows(2).all(|w| w[0].timestamp <= w[1].timestamp),
            "Deltas only reconstruct in time order, so '{}' must be ascending",
            node.node_name
        );
    }

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
        format_history(&out)?,
        "
metrics: lines, size

app
  t+0s  g1  OVER_THRESHOLD|ANCHOR  lines=20  size=300  (unattributable)
util
  t+0s  g1  OVER_THRESHOLD         lines=-  size=-  (unattributable)
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
        format_history(&out)?,
        "
metrics: (none)

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

/// Decode the response and render it, with timestamps normalized to an offset
/// from the earliest sample so the snapshot doesn't depend on the wall clock
/// the fixture ingested against.
fn format_history(out: &GetHistoryOutput) -> Result<String> {
    let metrics = match out.metrics.is_empty() {
        true => "(none)".to_owned(),
        false => out.metrics.join(", "),
    };
    let decoded = out
        .series
        .iter()
        .map(|node| Ok((node.node_name.as_str(), decode_series(&out.metrics, node)?)))
        .collect::<Result<Vec<_>>>()?;
    let base = decoded
        .iter()
        .flat_map(|(_, samples)| samples.first())
        .map(|sample| sample.timestamp)
        .min()
        .unwrap_or(0);

    let mut lines = vec![format!("metrics: {metrics}"), String::new()];
    for (node_name, samples) in &decoded {
        lines.push((*node_name).to_owned());
        if samples.is_empty() {
            lines.push("  (no samples)".to_owned());
            continue;
        }
        lines.extend(format_samples(&out.metrics, samples, base));
    }
    Ok(lines.join("\n"))
}

fn format_samples(metrics: &[String], samples: &[DecodedSample], base: i64) -> Vec<String> {
    let offsets = samples
        .iter()
        .map(|sample| format!("t+{}s", sample.timestamp - base))
        .collect::<Vec<_>>();
    let width = offsets.iter().map(String::len).max().unwrap_or(0);

    samples
        .iter()
        .zip(&offsets)
        .map(|(sample, offset)| {
            let values = metrics
                .iter()
                .zip(&sample.values)
                .map(|(name, value)| match value {
                    Some(value) => format!("{name}={value}"),
                    None => format!("{name}=-"),
                })
                .collect::<Vec<_>>()
                .join("  ");
            let step = match sample.attributable {
                true => "",
                false => "  (unattributable)",
            };
            format!(
                "  {offset:<width$}  g{}  {:<22} {values}{step}",
                sample.graph_id,
                sample.reasons.to_string(),
            )
        })
        .collect()
}

/// The raw stream, chunked at the stride. The leading absolute timestamp is
/// masked as `T0` — it is the one value that tracks the wall clock; every
/// delta after it is stable.
fn format_wire(out: &GetHistoryOutput) -> String {
    let stride = SAMPLE_HEADER_LEN + out.metrics.len();
    let width = out
        .series
        .iter()
        .map(|node| node.node_name.len())
        .max()
        .unwrap_or(0);

    let mut lines = vec![
        format!(
            "stride {stride} = {SAMPLE_HEADER_LEN} header + {} metrics",
            out.metrics.len()
        ),
        String::new(),
    ];
    for node in &out.series {
        let chunks = node
            .deltas
            .chunks(stride)
            .enumerate()
            .map(|(index, chunk)| format_chunk(index, chunk))
            .collect::<Vec<_>>()
            .join("  |  ");
        lines.push(format!("{:<width$}  {chunks}", node.node_name));
    }
    lines.join("\n")
}

fn format_chunk(index: usize, chunk: &[Option<f64>]) -> String {
    chunk
        .iter()
        .enumerate()
        .map(|(column, value)| match (index, column, value) {
            (0, 0, _) => "T0".to_owned(),
            (_, _, Some(value)) => format!("{value}"),
            (_, _, None) => "null".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(", ")
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
                lookback_hours: None,
                threshold: 1.0,
                graph_id_bounds: (None, None),
            },
            &t.task,
        )
        .await?;

    Ok(timeline_id.to_owned())
}

/// Three frames whose middle one falls under the threshold, so history keeps it
/// only as the anchor for the crossing that follows.
///
/// The steps are +45 then +105 against a bar of 50: the first is not one diff's
/// worth of movement and records nothing, the second is, and it drags the frame
/// before it along so its step reads as +105 rather than +150.
async fn ingest_sparse_history(t: &TestApp) -> Result<String> {
    let timeline_id = "history_sparse_test";
    let tid = TimelineID(timeline_id.to_owned());
    t.app
        .db
        .timelines
        .create(&tid, &default_timeline_config(), &t.task)
        .await?;

    store_frame(t, &tid, 0, 0, &[("app", 10.0, 100.0)]).await?;
    store_frame(t, &tid, 1, 10, &[("app", 10.0, 145.0)]).await?;
    store_frame(t, &tid, 2, 20, &[("app", 10.0, 250.0)]).await?;

    t.app
        .db
        .graph_history
        .ingest(
            &tid,
            &HistoryIngestOptions {
                lookback_hours: None,
                threshold: 50.0,
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

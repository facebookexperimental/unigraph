// Copyright (c) Meta Platforms, Inc. and affiliates.

//! End-to-end tests for `unigraph history`, against the real SQLite backend.
//!
//! The suite is organised around the three things that are hard about this
//! subsystem, and one that used to be:
//!
//! - **What a threshold means.** A row is kept where the node moved against the
//!   *immediately preceding built frame*. Slow creep is deliberately not
//!   recorded.
//! - **Holes.** The source timeline is built out of order, so gaps open and
//!   close constantly. A hole must cost two boundary rows and a re-judgement of
//!   one frame — and nothing else.
//! - **Convergence.** However scrambled the fill order, the end state must be
//!   the one a clean in-order ingest would have produced.
//! - **Recovery** *(the one that used to be hard)*. A frame missed by an
//!   outage must still be picked up, however long ago it was.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::LazyLock;

use anyhow::Result;
use unigraph_core::ArrayGraphNodes;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializableEdges;
use unigraph_core::ArrayGraphSerializableNodeMetadata;
use unigraph_db::GraphRangeBuilder;
use unigraph_db::HistoryCompactOptions;
use unigraph_db::HistoryIngestOptions;
use unigraph_db::UnigraphDb;
use unigraph_db::graph_history::FrameFlags;
use unigraph_db::graph_history::IngestState;
use unigraph_db::graph_history::MAX_ATTEMPTS;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_timestamp::Timestamp;

// ── Fixtures ─────────────────────────────────────────────────

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

/// Same as [`make_db`], but keeps the backend handle so tests can poke at blob
/// storage directly (to inject fetch failures, verify sweeps, ...).
fn make_db_with_storage() -> (UnigraphDb, Arc<SqliteStorage>) {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    (UnigraphDb::new(sqlite.clone(), sqlite.clone()), sqlite)
}

async fn setup_timeline(db: &UnigraphDb, name: &str, task: &ll::Task) -> Result<TimelineID> {
    setup_timeline_with_blobs(db, name, BlobStorageMode::Inline, task).await
}

async fn setup_timeline_with_blobs(
    db: &UnigraphDb,
    name: &str,
    blob_storage: BlobStorageMode,
    task: &ll::Task,
) -> Result<TimelineID> {
    let timeline_id = TimelineID(name.to_string());
    db.timelines
        .create(
            &timeline_id,
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage,
                store_metric_history: None,
            },
            task,
        )
        .await?;
    Ok(timeline_id)
}

async fn store_metric_graph(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_id: i64,
    timestamp: Timestamp,
    metrics: &[(&str, f64)],
    task: &ll::Task,
) -> Result<()> {
    db.graph
        .store(
            &GraphTimeKey {
                timeline_id: timeline_id.clone(),
                graph_id: GraphID(graph_id),
                timestamp,
            },
            &one_node_graph(metrics),
            None,
            task,
        )
        .await
}

async fn store_multi_node_graph(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_id: i64,
    timestamp: Timestamp,
    nodes: &[(&str, f64)],
    task: &ll::Task,
) -> Result<()> {
    db.graph
        .store(
            &GraphTimeKey {
                timeline_id: timeline_id.clone(),
                graph_id: GraphID(graph_id),
                timestamp,
            },
            &multi_node_graph(nodes),
            None,
            task,
        )
        .await
}

/// A graph with one `size` metric per named node.
fn multi_node_graph(nodes: &[(&str, f64)]) -> ArrayGraphSerializable {
    let mut names = String::new();
    let mut offsets = vec![0usize];
    for (name, _) in nodes {
        names.push_str(name);
        offsets.push(names.len());
    }
    let sizes = nodes.iter().map(|(_, value)| *value).collect::<Vec<_>>();

    ArrayGraphSerializable {
        node_names_ordered: ArrayGraphNodes::from_parts(names, offsets),
        edges: ArrayGraphSerializableEdges {
            edges: vec![],
            edge_offsets: vec![0; nodes.len() + 1],
            edge_metadata: vec![],
            edge_metadata_map: BTreeMap::new(),
        },
        node_metadata: ArrayGraphSerializableNodeMetadata {
            metrics: BTreeMap::from([("size".to_string(), sizes)]),
            labels: BTreeMap::new(),
            properties: BTreeMap::new(),
        },
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
        properties: BTreeMap::new(),
    }
}

fn one_node_graph(metrics: &[(&str, f64)]) -> ArrayGraphSerializable {
    let metric_values = metrics
        .iter()
        .map(|(name, value)| (name.to_string(), vec![*value]))
        .collect::<BTreeMap<_, _>>();
    ArrayGraphSerializable {
        node_names_ordered: ArrayGraphNodes::from_parts("app".to_string(), vec![0, 3]),
        edges: ArrayGraphSerializableEdges {
            edges: vec![],
            edge_offsets: vec![0, 0],
            edge_metadata: vec![],
            edge_metadata_map: BTreeMap::new(),
        },
        node_metadata: ArrayGraphSerializableNodeMetadata {
            metrics: metric_values,
            labels: BTreeMap::new(),
            properties: BTreeMap::new(),
        },
        graph_settings: None,
        traversal_config: None,
        entry_points: None,
        properties: BTreeMap::new(),
    }
}

/// Wall clock read once per process.
///
/// A frame's timestamp has to be identical whether it is computed when the
/// placeholder is registered or later when the frame is filled. Reading the
/// clock at each call made a frame filled in a later phase pick up a *later*
/// timestamp than a higher-numbered frame, which silently reorders the
/// timeline — frames are selected by `(timestamp, graph_id)`.
static TEST_NOW: LazyLock<i64> = LazyLock::new(|| Timestamp::now().to_unix_timestamp());

fn recent_timestamp(offset_secs: u64) -> Result<Timestamp> {
    let offset = i64::try_from(offset_secs)?;
    Ok(Timestamp::from_unix_timestamp(*TEST_NOW - 100 + offset))
}

/// A timestamp well outside any small `--lookback-hours` window.
fn old_timestamp() -> Timestamp {
    Timestamp::from_unix_timestamp(Timestamp::now().to_unix_timestamp() - 10 * 24 * 60 * 60)
}

const UNBOUNDED: TimestampBounds = TimestampBounds {
    start: None,
    end: None,
};

/// The normal shape: no lookback bound at all, which is what makes the work
/// list total.
fn ingest_opts(threshold: f64) -> HistoryIngestOptions {
    HistoryIngestOptions {
        lookback_hours: None,
        threshold,
        graph_id_bounds: (None, None),
        metrics: None,
    }
}

fn compact_opts(threshold: f64) -> HistoryCompactOptions {
    HistoryCompactOptions {
        threshold,
        range: HistoryRange::unbounded(),
    }
}

async fn ingest(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    threshold: f64,
    task: &ll::Task,
) -> Result<()> {
    db.graph_history
        .ingest(timeline_id, &ingest_opts(threshold), task)
        .await?;
    Ok(())
}

async fn compact(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    threshold: f64,
    task: &ll::Task,
) -> Result<()> {
    db.graph_history
        .compact(timeline_id, &compact_opts(threshold), task)
        .await?;
    Ok(())
}

/// Ingest then compact, which is how the scheduled pair actually runs.
async fn maintain(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    threshold: f64,
    task: &ll::Task,
) -> Result<()> {
    ingest(db, timeline_id, threshold, task).await?;
    compact(db, timeline_id, threshold, task).await
}

// ── Inspection ───────────────────────────────────────────────

async fn history_status(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_id: i64,
    task: &ll::Task,
) -> Result<HistoryStatusRow> {
    let mut conn = db.graph_conn().await?;
    conn.get_history_status(timeline_id, &[GraphID(graph_id)], task)
        .await?
        .pop()
        .ok_or_else(|| anyhow::anyhow!("no history status row for graph {graph_id}"))
}

async fn kept_graph_ids(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<Vec<i64>> {
    Ok(db
        .graph_history
        .series(timeline_id, "app", &UNBOUNDED, task)
        .await?
        .iter()
        .map(|row| row.graph_id.0)
        .collect())
}

/// One node's whole series: `graph_id:value reasons`, one row per line.
///
/// Every test here cares about *why* a row survived as much as *that* it did,
/// and the reasons are the whole model, so they go in the snapshot.
async fn series_summary(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    node_name: &str,
    task: &ll::Task,
) -> Result<String> {
    let rows = db
        .graph_history
        .series(timeline_id, node_name, &UNBOUNDED, task)
        .await?;
    if rows.is_empty() {
        return Ok("(no rows)".to_owned());
    }
    Ok(rows
        .iter()
        .map(|row| {
            let values = row
                .values
                .values()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join("/");
            let step = match row.attributable {
                true => "attributable",
                false => "—",
            };
            format!(
                "{:>3}  {:<6} {:<22} {step}",
                row.graph_id.0, values, row.reasons
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The whole timeline as three aligned strings, one character per frame.
///
/// ```text
/// frames  F Full   D Delta   X Error   . Empty (unbuilt)
/// state   I Ingested   P Pending   N NoData   ! Failed   . no checkpoint
/// flags   < before a gap   > after a gap   B both   g no data   . interior
/// ```
async fn timeline_state(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_ids: &[i64],
    task: &ll::Task,
) -> Result<String> {
    let mut frames = String::new();
    let mut states = String::new();
    let mut flags = String::new();

    for graph_id in graph_ids {
        let frame = db
            .frames
            .select(
                &FrameQuery {
                    timeline_id: timeline_id.clone(),
                    graph_ids: Some(vec![GraphID(*graph_id)]),
                    with_data: Some(false),
                    ..Default::default()
                },
                task,
            )
            .await?
            .pop();
        frames.push(frame.map_or(' ', |frame| frame_type_char(&frame.frame_type)));

        let status = history_status(db, timeline_id, *graph_id, task).await.ok();
        states.push(
            status
                .as_ref()
                .map_or('.', |status| match status.ingest_state {
                    IngestState::Ingested => 'I',
                    IngestState::Pending => 'P',
                    IngestState::NoData => 'N',
                    IngestState::Failed => '!',
                }),
        );
        flags.push(status.map_or('.', |status| flag_char(status.frame_flags)));
    }

    Ok(format!(
        "frames  {frames}\nstate   {states}\nflags   {flags}"
    ))
}

fn frame_type_char(frame_type: &FrameType) -> char {
    match frame_type {
        FrameType::Full => 'F',
        FrameType::Delta => 'D',
        FrameType::Error => 'X',
        FrameType::Empty => '.',
    }
}

fn flag_char(flags: FrameFlags) -> char {
    match flags {
        flags if flags.contains(FrameFlags::NO_DATA) => 'g',
        flags if flags.contains(FrameFlags::BARRIER) => 'B',
        flags if flags.contains(FrameFlags::AFTER_GAP) => '>',
        flags if flags.contains(FrameFlags::BEFORE_GAP) => '<',
        _ => '.',
    }
}

// ── Out-of-order fixtures ────────────────────────────────────

/// Register placeholders for `graph_ids` so later frames can be filled out of
/// order — the append-only check would reject inserting a lower ID afterwards.
async fn register_empty_frames(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_ids: &[i64],
    task: &ll::Task,
) -> Result<()> {
    let frames = graph_ids
        .iter()
        .map(|graph_id| {
            Ok(Frame {
                graph_id: GraphID(*graph_id),
                timestamp: recent_timestamp(u64::try_from(*graph_id)?)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(timeline_id, frames, false, task)
        .await?;
    Ok(())
}

/// Four frames of a node carrying both a `size` that steps +5/+5/+60 and a
/// `load_count` that jumps by thousands every frame — the shape a WWW budget
/// node has now that route load counts ride alongside tier sizes.
async fn fill_size_and_counter(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<()> {
    let frames = [
        (100.0, 10_000.0),
        (105.0, 23_000.0),
        (110.0, 4_000.0),
        (170.0, 51_000.0),
    ];
    for (index, (size, load_count)) in frames.iter().enumerate() {
        let graph_id = i64::try_from(index)? + 1;
        store_metric_graph(
            db,
            timeline_id,
            graph_id,
            recent_timestamp(u64::try_from(graph_id)?)?,
            &[("load_count", *load_count), ("size", *size)],
            task,
        )
        .await?;
    }
    Ok(())
}

/// Fill one already-registered placeholder with a real graph.
async fn fill_frame(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_id: i64,
    size: f64,
    task: &ll::Task,
) -> Result<()> {
    store_metric_graph(
        db,
        timeline_id,
        graph_id,
        recent_timestamp(u64::try_from(graph_id)?)?,
        &[("size", size)],
        task,
    )
    .await
}

/// Deterministic walk that drifts under `threshold` most steps and jumps past
/// it every fifth, so both the keep and the collapse paths get exercised.
///
/// The jump is scheduled rather than random, for a reason worth stating: under
/// per-frame semantics a walk of uniformly small steps crosses **nothing**,
/// however far it wanders in total — that is the slow-creep trade taken
/// deliberately. A fixture without guaranteed jumps quietly degenerates into
/// testing only the empty case, which is exactly what the first version of this
/// helper did.
///
/// Drift stays under 20 and jumps clear 40, so at a threshold of 25 the
/// crossings land on every fifth frame regardless of the seed's mood.
fn random_walk(count: usize) -> Vec<f64> {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut value = 1000.0f64;
    (0..count)
        .map(|index| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // High bits: the low ones of an xorshift are far too correlated to
            // drive a branch with.
            let magnitude = ((state >> 33) % 20) as f64;
            let direction = match (state >> 20) % 2 {
                0 => -1.0,
                _ => 1.0,
            };
            value += match index % 5 {
                4 => direction * (40.0 + magnitude),
                _ => direction * magnitude,
            };
            value
        })
        .collect()
}

/// The oracle: the same values ingested in ascending order into a clean
/// timeline, with no hole ever open.
async fn in_order_kept_ids_for(
    values: &[(i64, f64)],
    threshold: f64,
    name: &str,
    task: &ll::Task,
) -> Result<Vec<i64>> {
    let db = make_db();
    let timeline_id = setup_timeline(&db, name, task).await?;
    for (graph_id, value) in values {
        fill_frame(&db, &timeline_id, *graph_id, *value, task).await?;
    }
    maintain(&db, &timeline_id, threshold, task).await?;
    kept_graph_ids(&db, &timeline_id, task).await
}

// ── What a threshold means ───────────────────────────────────

/// The worked example from the redesign doc, end to end through storage.
///
/// Values 10, 10, 15, 15, 15, 20, 20, 21, 22, 23, 24, 29 at a threshold of 3.
/// Three things this pins at once:
///
/// - a crossing keeps the frame before it, so its step reads as one diff's work;
/// - frames 08–11 climb +1 each and record nothing, which is the slow-creep
///   trade taken deliberately;
/// - the newest frame is pinned regardless, so the right edge of a chart is the
///   truth rather than the last crossing.
#[tokio::test]
async fn graph_history_records_the_worked_example_from_the_design() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_worked_example", &task).await?;

    let values = [
        10.0, 10.0, 15.0, 15.0, 15.0, 20.0, 20.0, 21.0, 22.0, 23.0, 24.0, 29.0,
    ];
    for (index, value) in values.iter().enumerate() {
        fill_frame(&db, &timeline_id, i64::try_from(index)? + 1, *value, &task).await?;
    }
    maintain(&db, &timeline_id, 3.0, &task).await?;

    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "
  1  10     FIRST                  —
  2  10     ANCHOR                 attributable
  3  15     OVER_THRESHOLD         attributable
  5  15     ANCHOR                 —
  6  20     OVER_THRESHOLD         attributable
 11  24     ANCHOR                 —
 12  29     OVER_THRESHOLD|LATEST  attributable
"
    );
    Ok(())
}

/// The threshold is an OR across metrics, so a metric that churns for reasons
/// no diff caused defeats it on its own: WWW budget nodes carry a route's
/// 30-day load count next to its tier sizes, and no size threshold can be set
/// high enough to survive a counter in the tens of thousands.
///
/// `metrics` names what the series is about. Here `size` steps +5/+5/+60
/// against a bar of 50 — one crossing, at frame 4 — while `load_count` jumps by
/// thousands at every frame. Judged on both, every frame is a crossing and the
/// series is the whole timeline; judged on `size` alone it is the three rows
/// that mean something. The unrecorded metric is also absent from the stored
/// values, which is what the single-value rows below show.
#[tokio::test]
async fn graph_history_judges_only_the_metrics_it_was_told_to_record() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();

    // Every metric, which is what the option exists to escape.
    let noisy = setup_timeline(&db, "history_metrics_all", &task).await?;
    fill_size_and_counter(&db, &noisy, &task).await?;
    ingest(&db, &noisy, 50.0, &task).await?;
    k9::snapshot!(
        series_summary(&db, &noisy, "app", &task).await?,
        "
  1  10000/100 FIRST|ANCHOR           —
  2  23000/105 OVER_THRESHOLD|ANCHOR  attributable
  3  4000/110 OVER_THRESHOLD|ANCHOR  attributable
  4  51000/170 OVER_THRESHOLD|LATEST  attributable
"
    );

    // Size only. Same graphs, same threshold.
    let quiet = setup_timeline(&db, "history_metrics_size", &task).await?;
    fill_size_and_counter(&db, &quiet, &task).await?;
    db.graph_history
        .ingest(
            &quiet,
            &HistoryIngestOptions {
                metrics: Some(BTreeSet::from(["size".to_owned()])),
                ..ingest_opts(50.0)
            },
            &task,
        )
        .await?;
    k9::snapshot!(
        series_summary(&db, &quiet, "app", &task).await?,
        "
  1  100    FIRST                  —
  3  110    ANCHOR                 —
  4  170    OVER_THRESHOLD|LATEST  attributable
"
    );
    Ok(())
}

/// A node that creeps records nothing at all — no single diff moved it — and
/// the LATEST pin is what stops the chart's right edge going stale.
#[tokio::test]
async fn graph_history_does_not_record_slow_creep_but_still_pins_the_present() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_creep", &task).await?;

    for step in 0..20i64 {
        fill_frame(&db, &timeline_id, step + 1, 100.0 + step as f64, &task).await?;
    }
    maintain(&db, &timeline_id, 10.0, &task).await?;

    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "
  1  100    FIRST                  —
 20  119    LATEST                 —
"
    );
    Ok(())
}

/// A diff stack: consecutive frames each cross, so every row is a crossing and
/// the anchor for the one after it. The old design could not express that —
/// `anchor` meant "not a crossing" — and reported these steps as
/// unattributable.
#[tokio::test]
async fn graph_history_keeps_a_diff_stack_fully_attributable() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_diff_stack", &task).await?;

    for (index, value) in [10.0, 40.0, 70.0, 100.0].iter().enumerate() {
        fill_frame(&db, &timeline_id, i64::try_from(index)? + 1, *value, &task).await?;
    }
    maintain(&db, &timeline_id, 20.0, &task).await?;

    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "
  1  10     FIRST|ANCHOR           —
  2  40     OVER_THRESHOLD|ANCHOR  attributable
  3  70     OVER_THRESHOLD|ANCHOR  attributable
  4  100    OVER_THRESHOLD|LATEST  attributable
"
    );
    Ok(())
}

/// The LATEST pin moves forward rather than accumulating: only ever one row per
/// node carries it, and it is always the newest built frame.
#[tokio::test]
async fn graph_history_moves_the_latest_pin_forward_across_runs() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_latest_pin", &task).await?;

    let mut snapshots = Vec::new();
    for step in 0..3i64 {
        fill_frame(&db, &timeline_id, step + 1, 100.0 + step as f64, &task).await?;
        maintain(&db, &timeline_id, 50.0, &task).await?;
        snapshots.push(format!(
            "after frame {}\n{}",
            step + 1,
            series_summary(&db, &timeline_id, "app", &task).await?
        ));
    }

    k9::snapshot!(
        snapshots.join("\n\n"),
        "
after frame 1
  1  100    FIRST|LATEST           —

after frame 2
  1  100    FIRST                  —
  2  101    LATEST                 attributable

after frame 3
  1  100    FIRST                  —
  3  102    LATEST                 —
"
    );
    Ok(())
}

/// Every node in the graph is judged independently.
///
/// A node dropping out is a real event: it crosses the threshold on its way to
/// nothing, and the row it leaves carries no metrics at all — which is exactly
/// how the wire format spells "absent from this frame", as distinct from "this
/// frame was not recorded".
#[tokio::test]
async fn graph_history_judges_each_node_and_records_a_disappearance() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_multi_node", &task).await?;

    let frames: [&[(&str, f64)]; 3] = [
        &[("steady", 100.0), ("mover", 100.0), ("leaver", 100.0)],
        &[("steady", 101.0), ("mover", 500.0), ("leaver", 100.0)],
        &[("steady", 102.0), ("mover", 500.0)],
    ];
    for (index, nodes) in frames.iter().enumerate() {
        let graph_id = i64::try_from(index)? + 1;
        store_multi_node_graph(
            &db,
            &timeline_id,
            graph_id,
            recent_timestamp(u64::try_from(graph_id)?)?,
            nodes,
            &task,
        )
        .await?;
    }
    maintain(&db, &timeline_id, 50.0, &task).await?;

    let mut report = Vec::new();
    for node in ["steady", "mover", "leaver"] {
        report.push(format!(
            "{node}\n{}",
            series_summary(&db, &timeline_id, node, &task).await?
        ));
    }
    k9::snapshot!(
        report.join("\n\n"),
        "
steady
  1  100    FIRST                  —
  3  102    LATEST                 —

mover
  1  100    FIRST|ANCHOR           —
  2  500    OVER_THRESHOLD         attributable
  3  500    LATEST                 attributable

leaver
  1  100    FIRST                  —
  2  100    ANCHOR                 attributable
  3         OVER_THRESHOLD|LATEST  attributable
"
    );
    Ok(())
}

// ── Holes ────────────────────────────────────────────────────

/// A gap costs exactly two boundary rows, and the step across it is explicitly
/// not attributable to anything.
#[tokio::test]
async fn graph_history_bounds_a_gap_with_a_row_on_each_side() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_gap_barriers", &task).await?;
    register_empty_frames(&db, &timeline_id, &(1..=6).collect::<Vec<_>>(), &task).await?;

    // Frames 3 and 4 are never built.
    for (graph_id, value) in [(1i64, 100.0), (2, 100.0), (5, 900.0), (6, 900.0)] {
        fill_frame(&db, &timeline_id, graph_id, value, &task).await?;
    }
    maintain(&db, &timeline_id, 50.0, &task).await?;

    k9::snapshot!(
        timeline_state(&db, &timeline_id, &(1..=6).collect::<Vec<_>>(), &task).await?,
        "
frames  FF..FF
state   IINNII
flags   .<gg>.
"
    );
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "
  1  100    FIRST                  —
  2  100    -                      attributable
  5  900    -                      —
  6  900    LATEST                 attributable
"
    );
    Ok(())
}

/// An `Error` frame is a gap, exactly like an unbuilt one. The diff landed and
/// changed the code; we simply have no value for it, which is just as unknown.
#[tokio::test]
async fn graph_history_treats_a_failed_build_as_a_gap() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_error_gap", &task).await?;

    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    db.graph
        .store_error(
            &GraphTimeKey {
                timeline_id: timeline_id.clone(),
                graph_id: GraphID(2),
                timestamp: recent_timestamp(2)?,
            },
            &[TimestampedError {
                timestamp: recent_timestamp(2)?,
                message: "the source build failed".to_owned(),
            }],
            &task,
        )
        .await?;
    fill_frame(&db, &timeline_id, 3, 101.0, &task).await?;
    maintain(&db, &timeline_id, 50.0, &task).await?;

    k9::snapshot!(
        timeline_state(&db, &timeline_id, &[1, 2, 3], &task).await?,
        "
frames  FXF
state   INI
flags   <g>
"
    );
    Ok(())
}

/// Filling a hole touches its two neighbours and the frame itself, and lands on
/// exactly the series a clean in-order ingest produces. Nothing else in the
/// timeline moves.
#[tokio::test]
async fn graph_history_converges_when_a_gap_closes() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_gap_closes", &task).await?;
    register_empty_frames(&db, &timeline_id, &(1..=5).collect::<Vec<_>>(), &task).await?;

    let values = [
        (1i64, 100.0),
        (2, 100.0),
        (3, 100.0),
        (4, 200.0),
        (5, 200.0),
    ];
    for (graph_id, value) in values {
        if graph_id == 3 {
            continue;
        }
        fill_frame(&db, &timeline_id, graph_id, value, &task).await?;
    }
    maintain(&db, &timeline_id, 50.0, &task).await?;
    let open = series_summary(&db, &timeline_id, "app", &task).await?;

    fill_frame(&db, &timeline_id, 3, 100.0, &task).await?;
    maintain(&db, &timeline_id, 50.0, &task).await?;
    let closed = series_summary(&db, &timeline_id, "app", &task).await?;

    k9::snapshot!(
        format!("while the hole is open\n{open}\n\nonce it fills\n{closed}"),
        "
while the hole is open
  1  100    FIRST                  —
  2  100    -                      attributable
  4  200    -                      —
  5  200    LATEST                 attributable

once it fills
  1  100    FIRST                  —
  3  100    ANCHOR                 —
  4  200    OVER_THRESHOLD         attributable
  5  200    LATEST                 attributable
"
    );
    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        in_order_kept_ids_for(&values, 50.0, "history_gap_closes_oracle", &task).await?,
        "a filled hole must land on the in-order answer"
    );
    Ok(())
}

// ── Recovery ─────────────────────────────────────────────────

/// The wedge that used to be permanent.
///
/// A frame that is unbuilt when history first sweeps past it gets checkpointed,
/// and that checkpoint is the thing the old design treated as final — so a
/// frame filled after the lookback window had moved on was never reconsidered,
/// and froze compaction behind it for good. Here the work list has no time
/// bound at all, so an ingest run days later still finds it.
#[tokio::test]
async fn graph_history_ingests_a_frame_that_filled_long_after_the_sweep() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_late_fill", &task).await?;

    // Registered with old timestamps, so no lookback window would reach them.
    let frames = (1..=3)
        .map(|graph_id| Frame {
            graph_id: GraphID(graph_id),
            timestamp: old_timestamp(),
        })
        .collect::<Vec<_>>();
    db.graph
        .adjacent_deltas
        .put_new_empty_frames(&timeline_id, frames, false, &task)
        .await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        old_timestamp(),
        &[("size", 100.0)],
        &task,
    )
    .await?;
    maintain(&db, &timeline_id, 50.0, &task).await?;
    let before = timeline_state(&db, &timeline_id, &[1, 2, 3], &task).await?;

    // Days later, the source pipeline catches up.
    store_metric_graph(
        &db,
        &timeline_id,
        2,
        old_timestamp(),
        &[("size", 400.0)],
        &task,
    )
    .await?;
    store_metric_graph(
        &db,
        &timeline_id,
        3,
        old_timestamp(),
        &[("size", 400.0)],
        &task,
    )
    .await?;
    maintain(&db, &timeline_id, 50.0, &task).await?;

    k9::snapshot!(
        format!(
            "{before}\n\nafter the late fill\n{}\n\n{}",
            timeline_state(&db, &timeline_id, &[1, 2, 3], &task).await?,
            series_summary(&db, &timeline_id, "app", &task).await?
        ),
        "
frames  F..
state   INN
flags   <gg

after the late fill
frames  FFF
state   III
flags   ...

  1  100    FIRST|ANCHOR           —
  2  400    OVER_THRESHOLD         attributable
  3  400    LATEST                 attributable
"
    );
    Ok(())
}

/// Whatever order the holes fill in, the end state is the one a clean in-order
/// ingest produces.
///
/// One shape can pass by luck, so this sweeps several: sequential, workers
/// finishing back to front, round-robin interleaving, and a pathological
/// one-frame-per-wave descent. Compaction runs after every wave, as the
/// scheduled job does.
#[tokio::test]
async fn graph_history_any_fill_order_converges_on_the_in_order_result() -> Result<()> {
    let task = ll::Task::create_new("test");
    let threshold = 25.0;
    let walk = random_walk(12);
    let values = walk
        .iter()
        .enumerate()
        .map(|(index, value)| (i64::try_from(index).unwrap() + 1, *value))
        .collect::<Vec<_>>();

    let orders: [(&str, &[&[i64]]); 4] = [
        (
            "sequential",
            &[&[1, 2, 3, 4], &[5, 6, 7, 8], &[9, 10, 11, 12]],
        ),
        (
            "back to front",
            &[&[9, 10, 11, 12], &[5, 6, 7, 8], &[1, 2, 3, 4]],
        ),
        (
            "round robin",
            &[&[1, 4, 7, 10], &[2, 5, 8, 11], &[3, 6, 9, 12]],
        ),
        (
            "one at a time, descending",
            &[
                &[12],
                &[11],
                &[10],
                &[9],
                &[8],
                &[7],
                &[6],
                &[5],
                &[4],
                &[3],
                &[2],
                &[1],
            ],
        ),
    ];

    let expected =
        in_order_kept_ids_for(&values, threshold, "history_fill_order_oracle", &task).await?;
    let mut report = Vec::new();
    for (name, waves) in orders {
        let db = make_db();
        let timeline_id = setup_timeline(
            &db,
            &format!("history_fill_{}", name.replace(' ', "_")),
            &task,
        )
        .await?;
        register_empty_frames(&db, &timeline_id, &(1..=12).collect::<Vec<_>>(), &task).await?;

        for wave in waves {
            for graph_id in *wave {
                fill_frame(
                    &db,
                    &timeline_id,
                    *graph_id,
                    walk[*graph_id as usize - 1],
                    &task,
                )
                .await?;
            }
            maintain(&db, &timeline_id, threshold, &task).await?;
        }
        // One more pass: the last wave's fills can hand a neighbour back to the
        // work list, and that re-judgement lands on the next run.
        maintain(&db, &timeline_id, threshold, &task).await?;

        let kept = kept_graph_ids(&db, &timeline_id, &task).await?;
        assert_eq!(
            kept, expected,
            "filling '{name}' must converge on the in-order series"
        );
        report.push(format!("{name:<26} {kept:?}"));
    }

    k9::snapshot!(
        report.join("\n"),
        "
sequential                 [1, 4, 5, 9, 10, 12]
back to front              [1, 4, 5, 9, 10, 12]
round robin                [1, 4, 5, 9, 10, 12]
one at a time, descending  [1, 4, 5, 9, 10, 12]
"
    );
    Ok(())
}

// ── Failure handling ─────────────────────────────────────────

/// A frame history cannot read is recorded as `Failed` with a payload blob, and
/// recovers cleanly once the underlying failure goes away.
#[tokio::test]
async fn graph_history_failed_frame_recovers_on_retry() -> Result<()> {
    let task = ll::Task::create_new("test");
    let (db, sqlite) = make_db_with_storage();
    let timeline_id =
        setup_timeline_with_blobs(&db, "history_retry", BlobStorageMode::External, &task).await?;

    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    let stashed = break_graph_fetch(&sqlite).await?;

    let failed = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(50.0), &task)
        .await?;
    assert_eq!(failed.errors, 1, "the fetch failure must be recorded");
    let status = history_status(&db, &timeline_id, 1, &task).await?;
    assert_eq!(status.ingest_state, IngestState::Failed);
    assert_eq!(status.attempts, 1);
    assert!(
        status.error_blob_key.is_some(),
        "a failure must leave something to read"
    );

    for (key, data) in stashed {
        sqlite.put_blob(&key, &data).await?;
    }
    let recovered = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(50.0), &task)
        .await?;
    assert_eq!(recovered.errors, 0);
    assert_eq!(recovered.ingested, 1);

    let status = history_status(&db, &timeline_id, 1, &task).await?;
    assert_eq!(status.ingest_state, IngestState::Ingested);
    assert_eq!(
        status.error_blob_key, None,
        "a recovered frame must not keep pointing at a stale failure"
    );
    Ok(())
}

/// Once a frame has failed `MAX_ATTEMPTS` times it stops being retried, so a
/// permanently broken frame cannot burn a graph fetch on every scheduled run.
/// It stays a gap, which is the honest description: we have no values there.
#[tokio::test]
async fn graph_history_stops_retrying_at_the_attempt_cap() -> Result<()> {
    let task = ll::Task::create_new("test");
    let (db, sqlite) = make_db_with_storage();
    let timeline_id =
        setup_timeline_with_blobs(&db, "history_attempt_cap", BlobStorageMode::External, &task)
            .await?;

    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    break_graph_fetch(&sqlite).await?;

    for _ in 0..MAX_ATTEMPTS {
        db.graph_history
            .ingest(&timeline_id, &ingest_opts(50.0), &task)
            .await?;
    }
    assert_eq!(
        history_status(&db, &timeline_id, 1, &task).await?.attempts,
        i64::from(MAX_ATTEMPTS)
    );

    let capped = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(50.0), &task)
        .await?;
    assert_eq!(
        (capped.errors, capped.skipped),
        (0, 1),
        "past the cap the frame is skipped rather than retried"
    );
    Ok(())
}

/// Delete every external graph blob so `fetch_graph` fails, returning the
/// removed `(key, data)` pairs so a caller can restore them.
async fn break_graph_fetch(sqlite: &Arc<SqliteStorage>) -> Result<Vec<(String, Vec<u8>)>> {
    let mut stashed = Vec::new();
    for key in sqlite.list_blobs("graphs/").await? {
        let data = sqlite.get_blob(&key).await?;
        sqlite.delete_blob(&key).await?;
        stashed.push((key, data));
    }
    anyhow::ensure!(
        !stashed.is_empty(),
        "External blob mode should have offloaded the graph's blobs"
    );
    Ok(stashed)
}

// ── Compaction ───────────────────────────────────────────────

/// Ingesting at a threshold must land on exactly the same rows as ingesting
/// everything and compacting down to that threshold afterwards.
#[tokio::test]
async fn graph_history_ingest_matches_compaction() -> Result<()> {
    let task = ll::Task::create_new("test");
    let threshold = 25.0;
    let values = random_walk(40);

    let direct = make_db();
    let direct_id = setup_timeline(&direct, "history_direct", &task).await?;
    let compacted = make_db();
    let compacted_id = setup_timeline(&compacted, "history_compacted", &task).await?;

    for (index, value) in values.iter().enumerate() {
        let graph_id = i64::try_from(index)? + 1;
        fill_frame(&direct, &direct_id, graph_id, *value, &task).await?;
        fill_frame(&compacted, &compacted_id, graph_id, *value, &task).await?;
    }

    maintain(&direct, &direct_id, threshold, &task).await?;
    ingest(&compacted, &compacted_id, 0.0, &task).await?;
    compact(&compacted, &compacted_id, threshold, &task).await?;

    let direct_ids = kept_graph_ids(&direct, &direct_id, &task).await?;
    assert!(
        direct_ids.len() > 4 && direct_ids.len() < values.len(),
        "the walk should cross the threshold several times but not every step: {direct_ids:?}"
    );
    assert_eq!(
        direct_ids,
        kept_graph_ids(&compacted, &compacted_id, &task).await?,
        "re-thresholding down to a bar must match ingesting at it"
    );

    let second = compacted
        .graph_history
        .compact(&compacted_id, &compact_opts(threshold), &task)
        .await?;
    assert_eq!(
        (second.dropped, second.updated, second.collapsed),
        (0, 0, 0),
        "compaction is idempotent"
    );
    Ok(())
}

/// Raising the threshold retracts crossings and the anchors that existed only
/// to explain them, and leaves the position-shaped reasons alone.
#[tokio::test]
async fn graph_history_compaction_can_raise_the_threshold() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_raise_threshold", &task).await?;

    for (index, value) in [10.0, 40.0, 45.0, 200.0].iter().enumerate() {
        fill_frame(&db, &timeline_id, i64::try_from(index)? + 1, *value, &task).await?;
    }
    maintain(&db, &timeline_id, 20.0, &task).await?;
    let low = series_summary(&db, &timeline_id, "app", &task).await?;

    compact(&db, &timeline_id, 100.0, &task).await?;
    let high = series_summary(&db, &timeline_id, "app", &task).await?;

    k9::snapshot!(
        format!("at threshold 20\n{low}\n\nre-thresholded to 100\n{high}"),
        "
at threshold 20
  1  10     FIRST|ANCHOR           —
  2  40     OVER_THRESHOLD         attributable
  3  45     ANCHOR                 attributable
  4  200    OVER_THRESHOLD|LATEST  attributable

re-thresholded to 100
  1  10     FIRST                  —
  3  45     ANCHOR                 —
  4  200    OVER_THRESHOLD|LATEST  attributable
"
    );
    Ok(())
}

// ── Deletion ─────────────────────────────────────────────────

#[tokio::test]
async fn graph_history_delete_removes_rows() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_delete", &task).await?;

    for (index, value) in [100.0, 200.0, 300.0].iter().enumerate() {
        fill_frame(&db, &timeline_id, i64::try_from(index)? + 1, *value, &task).await?;
    }
    maintain(&db, &timeline_id, 50.0, &task).await?;
    assert!(!kept_graph_ids(&db, &timeline_id, &task).await?.is_empty());

    let report = db
        .graph_history
        .delete(&timeline_id, &(None, None), &task)
        .await?;
    assert!(report.entries_deleted > 0 && report.statuses_deleted > 0);
    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        Vec::<i64>::new()
    );

    // And a re-ingest rebuilds it from scratch, which is the documented
    // migration path.
    maintain(&db, &timeline_id, 50.0, &task).await?;
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "
  1  100    FIRST|ANCHOR           —
  2  200    OVER_THRESHOLD|ANCHOR  attributable
  3  300    OVER_THRESHOLD|LATEST  attributable
"
    );
    Ok(())
}

/// Wiping one timeline's history must not touch another's.
///
/// Graph IDs are allocated per timeline, so two timelines' histories overlap
/// completely in the `graph_id` space that `history delete` chunks over — the
/// only thing separating them is the `timeline_id` predicate on every
/// statement. Both timelines here are ingested at the same IDs on purpose, so a
/// missing predicate anywhere in the delete path takes the bystander with it.
#[tokio::test]
async fn graph_history_delete_is_scoped_to_one_timeline() -> Result<()> {
    let task = ll::Task::create_new("test");
    let db = make_db();
    let doomed = setup_timeline(&db, "history_doomed", &task).await?;
    let bystander = setup_timeline(&db, "history_bystander", &task).await?;

    for timeline_id in [&doomed, &bystander] {
        for (index, value) in [100.0, 400.0, 900.0].iter().enumerate() {
            fill_frame(&db, timeline_id, i64::try_from(index)? + 1, *value, &task).await?;
        }
        maintain(&db, timeline_id, 50.0, &task).await?;
    }

    db.graph_history
        .delete(&doomed, &(None, None), &task)
        .await?;

    assert_eq!(
        kept_graph_ids(&db, &doomed, &task).await?,
        Vec::<i64>::new()
    );
    assert_eq!(
        kept_graph_ids(&db, &bystander, &task).await?,
        vec![1, 2, 3],
        "the other timeline's history must be untouched"
    );
    Ok(())
}

/// `delete` registers every error blob in the range for cleanup, and a sweep
/// then physically removes them.
#[tokio::test]
async fn graph_history_delete_registers_error_blobs_for_sweep() -> Result<()> {
    let task = ll::Task::create_new("test");
    let (db, sqlite) = make_db_with_storage();
    let timeline_id =
        setup_timeline_with_blobs(&db, "history_blob_sweep", BlobStorageMode::External, &task)
            .await?;

    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    break_graph_fetch(&sqlite).await?;
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(50.0), &task)
        .await?;

    let blob_key = history_status(&db, &timeline_id, 1, &task)
        .await?
        .error_blob_key
        .ok_or_else(|| anyhow::anyhow!("the failed frame should have left a payload"))?;
    assert!(sqlite.has_blob(&blob_key).await?);

    let report = db
        .graph_history
        .delete(&timeline_id, &(None, None), &task)
        .await?;
    assert_eq!(report.error_blobs_registered, 1);

    db.blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await?;
    assert!(
        !sqlite.has_blob(&blob_key).await?,
        "a swept error payload must actually be gone"
    );
    Ok(())
}

// ── Delta chains ─────────────────────────────────────────────

/// Ingest reconstructs a whole chunk in one `load_range` + replay pass instead
/// of fetching each frame, so nothing pins the two against each other. Every
/// other test here stores Full frames only, which never exercises delta
/// application at all.
///
/// This builds a real `Full → Δ → Δ → …` chain via `store_range` and checks the
/// resulting history matches the same values ingested from standalone Full
/// frames. If replay ever diverges from `fetch_graph`, the series diverge.
#[tokio::test]
async fn graph_history_ingest_matches_across_a_real_delta_chain() -> Result<()> {
    let task = ll::Task::create_new("test");
    let threshold = 25.0;
    let walk = random_walk(20);

    let chained = make_db();
    let chained_id = setup_timeline(&chained, "history_delta_chain", &task).await?;
    store_as_delta_chain(&chained, &chained_id, &walk, &task).await?;

    let frame_types = chained
        .frames
        .select(
            &FrameQuery {
                timeline_id: chained_id.clone(),
                with_data: Some(false),
                order: Some(Order::Asc),
                ..Default::default()
            },
            &task,
        )
        .await?
        .iter()
        .map(|row| frame_type_char(&row.frame_type))
        .collect::<String>();
    k9::snapshot!(frame_types, "FDDDDDDDDDDDDDDDDDDD");

    maintain(&chained, &chained_id, threshold, &task).await?;

    let values = walk
        .iter()
        .enumerate()
        .map(|(index, value)| (i64::try_from(index).unwrap() + 1, *value))
        .collect::<Vec<_>>();
    assert_eq!(
        kept_graph_ids(&chained, &chained_id, &task).await?,
        in_order_kept_ids_for(&values, threshold, "history_delta_chain_oracle", &task).await?,
        "replaying a delta chain must produce the same series as standalone Full frames"
    );
    Ok(())
}

/// An ingest window almost never lines up with a chain head — a `graph_id`
/// bound picked for a repair, or (in production) a fixed-size replay chunk,
/// starts wherever it starts, and on a long chain that is a Delta.
///
/// The replay path handles that by reaching back to the Full the chain hangs
/// off. Before it did, every such window failed its `load_range` and quietly
/// fell back to fetching one frame at a time: same numbers, O(L²) work. So the
/// series equality below is the weaker half of this test — `replay_fallbacks`
/// is the half that fails if the reach-back goes away.
#[tokio::test]
async fn graph_history_ingest_replays_a_window_whose_full_is_outside_it() -> Result<()> {
    let task = ll::Task::create_new("test");
    let threshold = 25.0;
    let walk = random_walk(20);
    // Graph 1 carries the only Full, so any window starting past it opens on a
    // Delta.
    let mut options = ingest_opts(threshold);
    options.graph_id_bounds = (Some(GraphID(8)), None);

    let chained = make_db();
    let chained_id = setup_timeline(&chained, "history_window_past_full", &task).await?;
    store_as_delta_chain(&chained, &chained_id, &walk, &task).await?;

    let report = chained
        .graph_history
        .ingest(&chained_id, &options, &task)
        .await?;
    assert!(
        report.ingested > 0,
        "the window must actually contain frames, or the fallback count says nothing"
    );
    assert_eq!(
        report.replay_fallbacks, 0,
        "a window opening on a Delta must still replay in one pass"
    );

    // The same values over the same window, but as standalone Full frames — no
    // chain to reach back through.
    let fulls = make_db();
    let fulls_id = setup_timeline(&fulls, "history_window_past_full_oracle", &task).await?;
    for (index, value) in walk.iter().enumerate() {
        fill_frame(&fulls, &fulls_id, i64::try_from(index)? + 1, *value, &task).await?;
    }
    fulls
        .graph_history
        .ingest(&fulls_id, &options, &task)
        .await?;

    assert_eq!(
        kept_graph_ids(&chained, &chained_id, &task).await?,
        kept_graph_ids(&fulls, &fulls_id, &task).await?,
        "a mid-chain window must produce the same series as standalone Full frames"
    );
    Ok(())
}

/// One stored range: a Full followed by deltas for everything after it.
async fn store_as_delta_chain(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    values: &[f64],
    task: &ll::Task,
) -> Result<()> {
    register_empty_frames(
        db,
        timeline_id,
        &(1..=i64::try_from(values.len())?).collect::<Vec<_>>(),
        task,
    )
    .await?;

    let mut builder = GraphRangeBuilder::new(timeline_id.clone());
    for (index, value) in values.iter().enumerate() {
        let graph_id = i64::try_from(index)? + 1;
        builder.add(
            GraphTimeKey {
                timeline_id: timeline_id.clone(),
                graph_id: GraphID(graph_id),
                timestamp: recent_timestamp(u64::try_from(graph_id)?)?,
            },
            one_node_graph(&[("size", *value)]),
        )?;
    }
    db.graph
        .adjacent_deltas
        .store_range(builder.finalize(), task)
        .await?;
    Ok(())
}

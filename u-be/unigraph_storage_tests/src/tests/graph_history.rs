// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
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
use unigraph_db::graph_history::ErrorPayload;
use unigraph_db::graph_history::HistoryStatus;
use unigraph_db::graph_history::MAX_ATTEMPTS;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_timestamp::Timestamp;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

/// Same as [`make_db`], but keeps the backend handle so tests can poke at
/// blob storage directly (to inject fetch failures, verify sweeps, ...).
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
    metrics: &[(&str, f32)],
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
    nodes: &[(&str, f32)],
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
fn multi_node_graph(nodes: &[(&str, f32)]) -> ArrayGraphSerializable {
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

fn one_node_graph(metrics: &[(&str, f32)]) -> ArrayGraphSerializable {
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

/// Ingest options for tests that only care about the threshold.
///
/// `settle_hours: 0` puts the settle cutoff at "now", so every frame in these
/// fixtures counts as settled and omission is never deferred — i.e. the plain
/// threshold behaviour. Tests that exercise the out-of-order path build their
/// own options with a real settle window.
fn ingest_opts(lookback_hours: usize, threshold: f64) -> HistoryIngestOptions {
    HistoryIngestOptions {
        lookback_hours,
        settle_hours: 0,
        threshold,
        graph_id_bounds: (None, None),
    }
}

fn compact_opts(threshold: f64) -> HistoryCompactOptions {
    HistoryCompactOptions {
        threshold,
        settle_hours: 0,
        range: HistoryRange::unbounded(),
        deferred_only: false,
    }
}

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

/// One node's whole series as `graph_id:value`, anchors marked.
///
/// Both what was kept and which rows are anchors matter to every test here, and
/// one line per series reads better than two parallel `Vec` assertions.
async fn series_summary(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    node_name: &str,
    task: &ll::Task,
) -> Result<String> {
    Ok(db
        .graph_history
        .series(timeline_id, node_name, &UNBOUNDED, task)
        .await?
        .iter()
        .map(|row| {
            let values = row
                .values
                .values()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join("/");
            let anchor = if row.anchor { " (anchor)" } else { "" };
            format!("{}:{values}{anchor}", row.graph_id.0)
        })
        .collect::<Vec<_>>()
        .join("  "))
}

#[tokio::test]
async fn graph_history_ingest_respects_threshold() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_threshold", &task).await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        recent_timestamp(0)?,
        &[("size", 100.0)],
        &task,
    )
    .await?;
    store_metric_graph(
        &db,
        &timeline_id,
        2,
        recent_timestamp(1)?,
        &[("size", 105.0)],
        &task,
    )
    .await?;
    store_metric_graph(
        &db,
        &timeline_id,
        3,
        recent_timestamp(2)?,
        &[("size", 111.0)],
        &task,
    )
    .await?;

    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    assert_eq!(report.processed, 2);
    assert_eq!(report.omitted, 1);
    assert_eq!(
        report.entries, 2,
        "anchors are not samples of their own frame"
    );
    assert_eq!(report.anchors, 1);

    // Graph 3 crossed the threshold against graph 1, but only +6 of that +11
    // is its own doing — which is exactly what the anchor at graph 2 records.
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  2:105 (anchor)  3:111"
    );
    Ok(())
}

#[tokio::test]
async fn graph_history_marks_empty_frames() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_empty", &task).await?;

    db.graph
        .adjacent_deltas
        .put_new_empty_frames(
            &timeline_id,
            vec![Frame {
                graph_id: GraphID(1),
                timestamp: recent_timestamp(0)?,
            }],
            false,
            &task,
        )
        .await?;

    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    assert_eq!(report.empty, 1);

    let mut conn = db.graph_conn().await?;
    let statuses = conn
        .get_history_status(&timeline_id, &[GraphID(1)], &task)
        .await?;
    assert_eq!(statuses.len(), 1);
    assert_eq!(
        statuses[0].status.parse::<HistoryStatus>()?,
        HistoryStatus::Empty
    );
    Ok(())
}

#[tokio::test]
async fn graph_history_compact_is_idempotent() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_compact", &task).await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        recent_timestamp(0)?,
        &[("size", 100.0)],
        &task,
    )
    .await?;
    store_metric_graph(
        &db,
        &timeline_id,
        2,
        recent_timestamp(1)?,
        &[("size", 105.0)],
        &task,
    )
    .await?;
    store_metric_graph(
        &db,
        &timeline_id,
        3,
        recent_timestamp(2)?,
        &[("size", 111.0)],
        &task,
    )
    .await?;

    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
        .await?;
    let first = db
        .graph_history
        .compact(&timeline_id, &compact_opts(10.0), &task)
        .await?;
    let second = db
        .graph_history
        .compact(&timeline_id, &compact_opts(10.0), &task)
        .await?;

    // Graph 2 is redundant at this threshold, but it is graph 3's immediate
    // predecessor, so it is demoted to an anchor rather than deleted.
    assert_eq!((first.dropped, first.anchored), (0, 1));
    assert_eq!(
        (second.dropped, second.anchored),
        (0, 0),
        "a second pass at the same threshold must change nothing"
    );
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  2:105 (anchor)  3:111"
    );
    Ok(())
}

/// An anchor whose sample stops surviving is reclaimed rather than left behind.
#[tokio::test]
async fn graph_history_compact_reclaims_orphaned_anchors() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_anchor_orphan", &task).await?;

    for (graph_id, size) in [(1, 100.0), (2, 105.0), (3, 111.0)] {
        store_metric_graph(
            &db,
            &timeline_id,
            graph_id,
            recent_timestamp(u64::try_from(graph_id)?)?,
            &[("size", size)],
            &task,
        )
        .await?;
    }
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  2:105 (anchor)  3:111"
    );

    // At this threshold graph 3 no longer clears the bar, so nothing needs the
    // anchor at graph 2 any more and both go.
    let report = db
        .graph_history
        .compact(&timeline_id, &compact_opts(1000.0), &task)
        .await?;
    assert_eq!((report.dropped, report.anchored), (2, 0));
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100"
    );
    Ok(())
}

#[tokio::test]
async fn graph_history_delete_removes_rows() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_delete", &task).await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        recent_timestamp(0)?,
        &[("size", 100.0)],
        &task,
    )
    .await?;
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
        .await?;

    let report = db
        .graph_history
        .delete(&timeline_id, &(None, None), &task)
        .await?;
    assert_eq!(report.entries_deleted, 1);
    assert_eq!(report.statuses_deleted, 1);
    assert_eq!(report.metrics_deleted, 1);

    let series = db
        .graph_history
        .series(
            &timeline_id,
            "app",
            &TimestampBounds {
                start: None,
                end: None,
            },
            &task,
        )
        .await?;
    assert!(series.is_empty());
    Ok(())
}

/// Frames outside the lookback window are left completely alone, and the
/// in-window run primes its last-kept values from the DB rather than treating
/// the first in-window frame as a brand-new node.
///
/// Also the anchor path a scheduled job actually hits: each run's window opens
/// on a frame whose predecessor was ingested by an earlier run, so the
/// predecessor's values have to be recovered from storage rather than carried
/// in memory. A production job ingesting one frame per pass would otherwise
/// never write an anchor at all.
#[tokio::test]
async fn graph_history_lookback_window_primes_from_earlier_entries() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_lookback", &task).await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        old_timestamp(),
        &[("size", 100.0)],
        &task,
    )
    .await?;
    let wide_lookback = 24 * 365;
    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(wide_lookback, 10.0), &task)
        .await?;
    assert_eq!(report.processed, 1, "the old frame is inside a wide window");

    // 105 is only +5 from the last kept value (100) recorded 10 days ago, so a
    // 1-hour run that primes correctly must omit it.
    store_metric_graph(
        &db,
        &timeline_id,
        2,
        recent_timestamp(0)?,
        &[("size", 105.0)],
        &task,
    )
    .await?;
    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    assert_eq!(report.omitted, 1, "primed from the pre-window value");
    assert_eq!(report.skipped, 0, "the old frame is outside the window");

    store_metric_graph(
        &db,
        &timeline_id,
        3,
        recent_timestamp(1)?,
        &[("size", 120.0)],
        &task,
    )
    .await?;
    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;

    assert_eq!(
        report.anchors, 1,
        "frame 2 was ingested by an earlier run, so its values had to be \
         recovered from its graph to anchor frame 3"
    );
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  2:105 (anchor)  3:120"
    );
    Ok(())
}

/// Priming resolves each node's own *latest* kept value when many nodes are
/// primed in one batch.
///
/// The batch query pairs `MAX(graph_id)` with a bare `metric_values` column,
/// which can go wrong two ways: the blob could come from an older row of the
/// same node, or from a different node entirely. Every node therefore gets
/// two kept rows far apart in value, and the final nudge is small enough that
/// either mistake yields an over-threshold delta and keeps a row that should
/// have been omitted.
#[tokio::test]
async fn graph_history_primes_each_node_from_its_own_latest_entry() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_multi_prime", &task).await?;

    // Two pre-window frames 100 apart, so every node ends up with two kept
    // rows and `MAX(graph_id)` actually has to choose.
    let baseline = [("aaa", 100.0), ("bbb", 9000.0), ("ccc", 30.0)];
    store_multi_node_graph(&db, &timeline_id, 1, old_timestamp(), &baseline, &task).await?;
    let moved = [("aaa", 200.0), ("bbb", 9100.0), ("ccc", 130.0)];
    store_multi_node_graph(
        &db,
        &timeline_id,
        2,
        old_timestamp().add_minutes(1)?,
        &moved,
        &task,
    )
    .await?;
    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(24 * 365, 10.0), &task)
        .await?;
    assert_eq!(report.entries, 6, "two kept rows for each of three nodes");

    // +5 from each node's graph_id=2 value, so under the threshold. Priming
    // off graph_id=1 would see +105; priming off another node, thousands.
    let nudged = [("aaa", 205.0), ("bbb", 9105.0), ("ccc", 135.0)];
    store_multi_node_graph(&db, &timeline_id, 3, recent_timestamp(0)?, &nudged, &task).await?;
    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    assert_eq!(report.omitted, 1);
    assert_eq!(
        report.entries, 0,
        "each node primed from its own latest entry"
    );

    for node in ["aaa", "bbb", "ccc"] {
        let series = db
            .graph_history
            .series(&timeline_id, node, &UNBOUNDED, &task)
            .await?;
        assert_eq!(
            series.iter().map(|row| row.graph_id.0).collect::<Vec<_>>(),
            vec![1, 2],
            "unexpected series for {node}"
        );
    }
    Ok(())
}

/// A frame whose graph can't be fetched is recorded as `Error` with a payload
/// blob, and recovers cleanly once the underlying failure goes away.
#[tokio::test]
async fn graph_history_failed_frame_recovers_on_retry() -> Result<()> {
    let (db, sqlite) = make_db_with_storage();
    let task = ll::Task::create_new("test");
    let timeline_id =
        setup_timeline_with_blobs(&db, "history_retry", BlobStorageMode::External, &task).await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        recent_timestamp(0)?,
        &[("size", 100.0)],
        &task,
    )
    .await?;
    let stashed = break_graph_fetch(&sqlite).await?;

    // Stay below the retry cap — past it the frame is deliberately abandoned.
    let attempts = i64::from(MAX_ATTEMPTS) - 2;
    for _ in 0..attempts {
        let report = db
            .graph_history
            .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
            .await?;
        assert_eq!(report.errors, 1);
    }

    let status = history_status(&db, &timeline_id, 1, &task).await?;
    assert_eq!(
        status.status.parse::<HistoryStatus>()?,
        HistoryStatus::Error
    );
    assert_eq!(status.attempts, attempts);
    let error_key = status
        .error_blob_key
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Error status should reference an error blob"))?;
    let payload: ErrorPayload = serde_json::from_slice(
        &sqlite
            .get_blob(&error_key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("error blob {error_key} is missing"))?,
    )?;
    assert!(!payload.messages.is_empty(), "payload records the failure");

    for (key, data) in &stashed {
        sqlite.put_blob(key, data).await?;
    }
    let report = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
        .await?;
    assert_eq!(report.processed, 1);

    let status = history_status(&db, &timeline_id, 1, &task).await?;
    assert_eq!(
        status.status.parse::<HistoryStatus>()?,
        HistoryStatus::Processed
    );
    assert_eq!(status.error_blob_key, None);
    assert!(
        db.blob_storage
            .get_pending_cleanup(&task)
            .await?
            .contains(&error_key),
        "the superseded error blob is queued for cleanup"
    );
    Ok(())
}

/// Once a frame has failed `MAX_ATTEMPTS` times it stops being retried, so a
/// permanently-broken frame can't burn a graph fetch on every scheduled run.
#[tokio::test]
async fn graph_history_stops_retrying_at_the_attempt_cap() -> Result<()> {
    let (db, sqlite) = make_db_with_storage();
    let task = ll::Task::create_new("test");
    let timeline_id =
        setup_timeline_with_blobs(&db, "history_retry_cap", BlobStorageMode::External, &task)
            .await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        recent_timestamp(0)?,
        &[("size", 100.0)],
        &task,
    )
    .await?;
    break_graph_fetch(&sqlite).await?;

    for _ in 0..MAX_ATTEMPTS {
        db.graph_history
            .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
            .await?;
    }
    let capped = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
        .await?;
    assert_eq!(capped.errors, 0, "no further fetch is attempted");
    assert_eq!(capped.skipped, 1);

    let status = history_status(&db, &timeline_id, 1, &task).await?;
    assert_eq!(
        status.status.parse::<HistoryStatus>()?,
        HistoryStatus::Error
    );
    assert_eq!(status.attempts, i64::from(MAX_ATTEMPTS));
    Ok(())
}

/// Delete every external graph blob so `fetch_graph` fails, returning the
/// removed `(key, data)` pairs so a caller can restore them.
async fn break_graph_fetch(sqlite: &Arc<SqliteStorage>) -> Result<Vec<(String, Vec<u8>)>> {
    let mut stashed = Vec::new();
    for key in sqlite.list_blobs("graphs/").await? {
        let data = sqlite
            .get_blob(&key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("listed blob {key} is missing"))?;
        sqlite.delete_blob(&key).await?;
        stashed.push((key, data));
    }
    anyhow::ensure!(
        !stashed.is_empty(),
        "External blob mode should have offloaded the graph's blobs"
    );
    Ok(stashed)
}

/// `delete` registers every error blob in the range for cleanup, and a sweep
/// then physically removes them.
#[tokio::test]
async fn graph_history_delete_registers_error_blobs_for_sweep() -> Result<()> {
    let (db, sqlite) = make_db_with_storage();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline_with_blobs(
        &db,
        "history_delete_blobs",
        BlobStorageMode::External,
        &task,
    )
    .await?;

    store_metric_graph(
        &db,
        &timeline_id,
        1,
        recent_timestamp(0)?,
        &[("size", 100.0)],
        &task,
    )
    .await?;
    break_graph_fetch(&sqlite).await?;
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, 0.0), &task)
        .await?;

    let error_key = history_status(&db, &timeline_id, 1, &task)
        .await?
        .error_blob_key
        .ok_or_else(|| anyhow::anyhow!("expected a failed frame"))?;
    assert!(sqlite.has_blob(&error_key).await?);

    let report = db
        .graph_history
        .delete(&timeline_id, &(None, None), &task)
        .await?;
    assert_eq!(report.statuses_deleted, 1);
    assert_eq!(report.error_blobs_registered, 1);

    db.blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await?;
    assert!(
        !sqlite.has_blob(&error_key).await?,
        "the sweep physically removes the error blob"
    );
    Ok(())
}

/// Ingesting at a threshold must land on exactly the same rows as ingesting
/// everything and compacting down to that threshold afterwards.
#[tokio::test]
async fn graph_history_randomized_ingest_matches_compaction() -> Result<()> {
    let task = ll::Task::create_new("test");
    let threshold = 25.0;
    let values = random_walk(40);

    let direct = make_db();
    let direct_id = setup_timeline(&direct, "history_direct", &task).await?;
    let compacted = make_db();
    let compacted_id = setup_timeline(&compacted, "history_compacted", &task).await?;

    for (i, value) in values.iter().enumerate() {
        let graph_id = i as i64 + 1;
        let timestamp = recent_timestamp(i as u64)?;
        let metrics = [("size", *value)];
        store_metric_graph(&direct, &direct_id, graph_id, timestamp, &metrics, &task).await?;
        store_metric_graph(
            &compacted,
            &compacted_id,
            graph_id,
            timestamp,
            &metrics,
            &task,
        )
        .await?;
    }

    direct
        .graph_history
        .ingest(&direct_id, &ingest_opts(1, threshold), &task)
        .await?;
    compacted
        .graph_history
        .ingest(&compacted_id, &ingest_opts(1, 0.0), &task)
        .await?;
    compacted
        .graph_history
        .compact(&compacted_id, &compact_opts(threshold), &task)
        .await?;

    let direct_ids = kept_graph_ids(&direct, &direct_id, &task).await?;
    assert!(
        direct_ids.len() > 1 && direct_ids.len() < values.len(),
        "the walk should cross the threshold sometimes but not every step: {direct_ids:?}"
    );
    assert_eq!(
        direct_ids,
        kept_graph_ids(&compacted, &compacted_id, &task).await?
    );

    let second = compacted
        .graph_history
        .compact(&compacted_id, &compact_opts(threshold), &task)
        .await?;
    assert_eq!(second.dropped, 0, "compaction is idempotent");
    Ok(())
}

/// Deterministic pseudorandom walk — steps are usually below `threshold` but
/// occasionally jump past it, so both keep and drop paths get exercised.
fn random_walk(count: usize) -> Vec<f32> {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut value = 100.0f32;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            value += (state % 41) as f32 - 20.0;
            value
        })
        .collect()
}

// -- Out-of-order fill --------------------------------------------------------
//
// The source timeline registers frames in `graph_id` order but builds them out
// of order, so history routinely sees a hole between two built frames. These
// tests cover the three consequences: a stamped-Empty frame must be ingestable
// once it fills, omission must be deferred behind a hole, and compaction must
// stop at the settled frontier.

/// A settle window wide enough that the fixtures' Empty frames still count as
/// "might still be filled".
const UNSETTLED_HOURS: usize = 24;

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

/// Fill one already-registered placeholder with a real graph.
async fn fill_frame(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_id: i64,
    size: f32,
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

fn deferred_ingest_opts(threshold: f64) -> HistoryIngestOptions {
    HistoryIngestOptions {
        lookback_hours: 1,
        settle_hours: UNSETTLED_HOURS,
        threshold,
        graph_id_bounds: (None, None),
    }
}

/// A frame stamped `Empty` before it was built must still be ingested once it
/// fills. Treating the stamp as final would permanently blacklist every frame
/// that happened to be unbuilt when history first swept past it.
#[tokio::test]
async fn graph_history_ingests_a_frame_that_filled_after_being_stamped_empty() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_refill", &task).await?;

    register_empty_frames(&db, &timeline_id, &[1], &task).await?;
    let stamped = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    assert_eq!(stamped.empty, 1, "the placeholder should be checkpointed");
    assert_eq!(
        history_status(&db, &timeline_id, 1, &task)
            .await?
            .status
            .parse::<HistoryStatus>()?,
        HistoryStatus::Empty
    );

    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    let refilled = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;

    assert_eq!(
        refilled.processed, 1,
        "a frame built after being stamped Empty must be re-ingested, not skipped forever"
    );
    assert_eq!(kept_graph_ids(&db, &timeline_id, &task).await?, vec![1]);
    Ok(())
}

/// A sample recorded while an earlier frame is still unbuilt cannot be omitted:
/// the frame that fills the hole could have changed the verdict, and omission
/// is permanent. Keep the row and flag it for `compact` instead.
#[tokio::test]
async fn graph_history_defers_omission_behind_an_unfilled_frame() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_deferred", &task).await?;

    // Frame 2 is left unbuilt, so frame 3 is judged across a hole.
    register_empty_frames(&db, &timeline_id, &[1, 2, 3], &task).await?;
    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    fill_frame(&db, &timeline_id, 3, 105.0, &task).await?;

    let report = db
        .graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;

    assert_eq!(
        report.deferred, 1,
        "frame 3 sits behind an unfilled frame, so its threshold must be deferred"
    );
    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        vec![1, 3],
        "the sub-threshold sample at frame 3 is kept rather than lost"
    );
    assert!(
        history_status(&db, &timeline_id, 3, &task)
            .await?
            .omission_deferred,
        "the checkpoint should record that compaction still owes work here"
    );
    Ok(())
}

/// Once the hole closes, `compact --deferred-only` re-applies the threshold to
/// exactly the flagged range and clears the flag.
#[tokio::test]
async fn graph_history_compact_reclaims_deferred_rows_once_the_gap_closes() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_reclaim", &task).await?;

    register_empty_frames(&db, &timeline_id, &[1, 2, 3], &task).await?;
    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    fill_frame(&db, &timeline_id, 3, 105.0, &task).await?;
    db.graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;

    // The hole closes, and frame 2 turns out to be unremarkable.
    fill_frame(&db, &timeline_id, 2, 103.0, &task).await?;
    db.graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;

    let report = db
        .graph_history
        .compact(
            &timeline_id,
            &HistoryCompactOptions {
                threshold: 10.0,
                settle_hours: 0,
                range: HistoryRange::unbounded(),
                deferred_only: true,
            },
            &task,
        )
        .await?;

    assert_eq!(report.dropped, 1);
    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        vec![1],
        "frame 3 is redundant against frame 1 once the whole range is settled"
    );
    assert!(
        !history_status(&db, &timeline_id, 3, &task)
            .await?
            .omission_deferred,
        "compaction should clear the flag it just acted on"
    );
    Ok(())
}

/// An anchor must never be mistaken for a baseline.
///
/// An anchor sits within a threshold of the sample that follows it by
/// construction, so measuring that sample against the anchor instead of the
/// last *surviving* row makes it look far smaller than it is. Compaction would
/// then delete the very sample the anchor exists to explain — and deleting a
/// row is as irreversible as never writing it.
///
/// Reaching that state takes a real out-of-order fixture: the anchor has to
/// land at a frame the earlier run omitted, and the sample it explains has to
/// be flagged, so that per-frame compaction resolves a baseline across it.
#[tokio::test]
async fn graph_history_anchor_is_never_a_baseline() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_anchor_baseline", &task).await?;

    // First run, nothing unsettled: frame 2 is +5 and gets omitted outright.
    register_empty_frames(&db, &timeline_id, &[1, 2, 3, 4], &task).await?;
    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    fill_frame(&db, &timeline_id, 2, 105.0, &task).await?;
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, 10.0), &task)
        .await?;
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100"
    );

    // Frame 4 lands behind the still-unbuilt frame 3, so it is kept and
    // flagged. It is +12 on the baseline at frame 1, and minting its anchor
    // brings frame 2 back — three graph IDs earlier than that baseline.
    fill_frame(&db, &timeline_id, 4, 112.0, &task).await?;
    let report = db
        .graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;
    assert_eq!(report.anchors, 1);
    assert!(
        history_status(&db, &timeline_id, 4, &task)
            .await?
            .omission_deferred
    );

    // Frame 4 must still be judged against frame 1 (+12, kept), not against
    // its own anchor at frame 2 (+7, which would delete it).
    let compacted = db
        .graph_history
        .compact(
            &timeline_id,
            &HistoryCompactOptions {
                threshold: 10.0,
                settle_hours: 0,
                range: HistoryRange::unbounded(),
                deferred_only: true,
            },
            &task,
        )
        .await?;
    assert_eq!(compacted.dropped, 0);
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  2:105 (anchor)  4:112"
    );
    Ok(())
}

/// The per-frame compaction path must keep a redundant row that is still the
/// immediate predecessor of a surviving sample, rather than deleting it.
///
/// This is the incremental path a scheduled job runs, and it sees one frame at
/// a time — so it has to look forward to the next built frame to know whether
/// anything still needs the row it is about to drop.
#[tokio::test]
async fn graph_history_deferred_compaction_keeps_a_surviving_samples_predecessor() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_deferred_anchor", &task).await?;

    // Frame 2 is left unbuilt, so frames 3 and 4 are judged across a hole and
    // every row is kept unconditionally.
    register_empty_frames(&db, &timeline_id, &[1, 2, 3, 4], &task).await?;
    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    fill_frame(&db, &timeline_id, 3, 105.0, &task).await?;
    fill_frame(&db, &timeline_id, 4, 150.0, &task).await?;
    db.graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  3:105  4:150"
    );

    // The hole closes and the flagged range is re-thresholded. Frame 3 is
    // redundant against frame 1 on its own, but frame 4 survives and frame 3 is
    // the frame right before it, so it stays as frame 4's anchor.
    fill_frame(&db, &timeline_id, 2, 101.0, &task).await?;
    db.graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;
    let report = db
        .graph_history
        .compact(
            &timeline_id,
            &HistoryCompactOptions {
                threshold: 10.0,
                settle_hours: 0,
                range: HistoryRange::unbounded(),
                deferred_only: true,
            },
            &task,
        )
        .await?;

    assert_eq!((report.dropped, report.anchored), (0, 1));
    k9::snapshot!(
        series_summary(&db, &timeline_id, "app", &task).await?,
        "1:100  3:105 (anchor)  4:150"
    );
    assert!(
        !history_status(&db, &timeline_id, 4, &task)
            .await?
            .omission_deferred,
        "compaction should clear the flag it just acted on"
    );
    Ok(())
}

/// Dropping a row is as irreversible as never writing it, so compaction must
/// stop at the first frame that could still change — even when rows beyond it
/// look redundant.
#[tokio::test]
async fn graph_history_compact_stops_at_the_settled_frontier() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "history_frontier", &task).await?;

    register_empty_frames(&db, &timeline_id, &[1, 2, 3, 4], &task).await?;
    fill_frame(&db, &timeline_id, 1, 100.0, &task).await?;
    fill_frame(&db, &timeline_id, 2, 102.0, &task).await?;
    fill_frame(&db, &timeline_id, 4, 104.0, &task).await?;
    db.graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(10.0), &task)
        .await?;

    let blocked = db
        .graph_history
        .compact(
            &timeline_id,
            &HistoryCompactOptions {
                threshold: 10.0,
                settle_hours: UNSETTLED_HOURS,
                range: HistoryRange::unbounded(),
                deferred_only: false,
            },
            &task,
        )
        .await?;
    assert_eq!(
        blocked.compacted_through,
        Some(GraphID(2)),
        "the unbuilt frame 3 is the frontier; nothing past it may be compacted"
    );
    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        vec![1, 4],
        "frame 4's deferred row survives while frame 3 could still appear"
    );

    // Age frame 3 out of the settle window: it is now presumed abandoned, the
    // frontier advances, and frame 4 can finally be judged.
    let aged = db
        .graph_history
        .compact(&timeline_id, &compact_opts(10.0), &task)
        .await?;
    assert_eq!(aged.compacted_through, Some(GraphID(4)));
    assert_eq!(kept_graph_ids(&db, &timeline_id, &task).await?, vec![1]);
    Ok(())
}

// -- End-to-end: the whole out-of-order pipeline -------------------------------

/// How many frames the end-to-end fixture registers.
const E2E_FRAMES: i64 = 14;
const E2E_THRESHOLD: f64 = 25.0;

/// Fill order, mimicking parallel `build-and-store-www` workers: contiguous
/// id ranges completing at very different times, one worker timing out
/// mid-range (11 lands long after 12..14), and one frame never built at all
/// (7 is absent — its source counterpart failed).
const E2E_WAVES: &[&[i64]] = &[
    &[1, 2, 3],    // worker A finishes first
    &[8, 9, 10],   // worker C finishes before B — a hole opens at 4..7
    &[12, 13, 14], // worker D finishes; 11 is still missing behind it
    &[4, 5, 6],    // worker B finally lands, closing most of the hole
    &[11],         // worker B's timed-out tail
];

/// The full out-of-order pipeline, end to end.
///
/// Registers a run of placeholders, then fills them in scrambled waves —
/// ingesting after each, as the scheduled job does — and snapshots the whole
/// timeline after every step. Frame 7 is never built, so it also covers a hole
/// that only closes by ageing out.
///
/// The snapshot is the readable artefact; the assertion at the end is the real
/// check: once everything has settled and compacted, the kept rows must be
/// exactly what a clean in-order ingest of the same values produces. Deferral
/// may keep extra rows in the meantime, but it must never change the answer.
#[tokio::test]
async fn graph_history_out_of_order_waves_converge_on_the_in_order_result() -> Result<()> {
    let task = ll::Task::create_new("test");
    let values = random_walk(usize::try_from(E2E_FRAMES)?);

    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_e2e", &task).await?;
    let all_ids = (1..=E2E_FRAMES).collect::<Vec<_>>();
    register_empty_frames(&db, &timeline_id, &all_ids, &task).await?;

    let mut table = vec![snapshot_header()];
    for (wave, ids) in E2E_WAVES.iter().enumerate() {
        fill_wave(&db, &timeline_id, ids, &values, &task).await?;
        let report = db
            .graph_history
            .ingest(&timeline_id, &deferred_ingest_opts(E2E_THRESHOLD), &task)
            .await?;
        table.push(
            snapshot_row(
                &db,
                &timeline_id,
                &format!("wave {wave}"),
                report.deferred,
                &task,
            )
            .await?,
        );
    }

    // Frame 7 never gets built. Dropping the settle window to zero ages it out,
    // which is what lets the frontier — and compaction — move past it.
    let settled = db
        .graph_history
        .ingest(&timeline_id, &ingest_opts(1, E2E_THRESHOLD), &task)
        .await?;
    table.push(snapshot_row(&db, &timeline_id, "aged out", settled.deferred, &task).await?);

    let compacted = db
        .graph_history
        .compact(&timeline_id, &compact_opts(E2E_THRESHOLD), &task)
        .await?;
    table.push(snapshot_row(&db, &timeline_id, "compacted", 0, &task).await?);
    assert_eq!(
        compacted.compacted_through,
        Some(GraphID(E2E_FRAMES)),
        "with frame 7 aged out, the frontier should reach the end of the timeline"
    );

    k9::snapshot!(
        table.join("\n"),
        "
step       frames         history        deferred kept rows
wave 0     FFF........... Pooeeeeeeeeeee 0        1
wave 1     FFF....FFF.... Pooeeee!!!eeee 3        1,8,9,10
wave 2     FFF....FFF.FFF Pooeeee!!!e!!! 3        1,8,9,10,12,13,14
wave 3     FFFFFF.FFF.FFF Poooooe!!!e!!! 0        1,8,9,10,12,13,14
wave 4     FFFFFF.FFFFFFF Poooooe!!!!!!! 1        1,8,9,10,11,12,13,14
aged out   FFFFFF.FFFFFFF Poooooe!!!!!!! 0        1,8,9,10,11,12,13,14
compacted  FFFFFF.FFFFFFF PoooooePPPPPPP 0        1,9*,10
"
    );

    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        in_order_kept_ids(&built_ids(E2E_WAVES), &values, "history_e2e_oracle", &task).await?,
        "scrambled fill order must converge on the same series as a clean in-order ingest"
    );
    assert_eq!(
        db.graph_history
            .compact(&timeline_id, &compact_opts(E2E_THRESHOLD), &task)
            .await?
            .dropped,
        0,
        "compaction is idempotent"
    );
    Ok(())
}

async fn fill_wave(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    graph_ids: &[i64],
    values: &[f32],
    task: &ll::Task,
) -> Result<()> {
    for graph_id in graph_ids {
        let value = values[usize::try_from(*graph_id)? - 1];
        fill_frame(db, timeline_id, *graph_id, value, task).await?;
    }
    Ok(())
}

/// The oracle: the same frames, ingested in ascending order with nothing ever
/// unsettled, so the threshold chain is the plain one.
///
/// Only the frames that actually got built are included — an unbuilt frame
/// contributes no sample either way, so comparing against a run that had one
/// would be measuring the wrong thing.
async fn in_order_kept_ids(
    built: &[i64],
    values: &[f32],
    timeline_name: &str,
    task: &ll::Task,
) -> Result<Vec<i64>> {
    let db = make_db();
    let timeline_id = setup_timeline(&db, timeline_name, task).await?;
    let mut ascending = built.to_vec();
    ascending.sort_unstable();
    for graph_id in ascending {
        fill_frame(
            &db,
            &timeline_id,
            graph_id,
            values[usize::try_from(graph_id)? - 1],
            task,
        )
        .await?;
    }
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, E2E_THRESHOLD), task)
        .await?;
    kept_graph_ids(&db, &timeline_id, task).await
}

/// How `run_fill_order` should reclaim what ingest had to over-keep.
#[derive(Clone, Copy)]
enum CompactMode {
    /// Per-frame, driven off the flagged checkpoints — what a scheduled job runs.
    DeferredOnly,
    /// Per-node re-derivation of the whole range — the threshold-change path.
    EveryNode,
}

/// Drive one fill ordering all the way through: register, fill in waves with an
/// ingest after each, age the never-built frames out, then compact.
///
/// Compaction runs after every wave, not just at the end — that is how the
/// scheduled job behaves, and it is the case where a per-frame pass has to get
/// its ordering right, since each run's deletions move the next run's baselines.
async fn run_fill_order(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    waves: &[&[i64]],
    values: &[f32],
    mode: CompactMode,
    task: &ll::Task,
) -> Result<()> {
    register_empty_frames(db, timeline_id, &(1..=E2E_FRAMES).collect::<Vec<_>>(), task).await?;
    for ids in waves {
        fill_wave(db, timeline_id, ids, values, task).await?;
        db.graph_history
            .ingest(timeline_id, &deferred_ingest_opts(E2E_THRESHOLD), task)
            .await?;
        compact_with(db, timeline_id, mode, UNSETTLED_HOURS, task).await?;
    }
    // Age the never-built frames out so the frontier can reach the end.
    db.graph_history
        .ingest(timeline_id, &ingest_opts(1, E2E_THRESHOLD), task)
        .await?;
    compact_with(db, timeline_id, mode, 0, task).await?;
    Ok(())
}

async fn compact_with(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    mode: CompactMode,
    settle_hours: usize,
    task: &ll::Task,
) -> Result<()> {
    db.graph_history
        .compact(
            timeline_id,
            &HistoryCompactOptions {
                threshold: E2E_THRESHOLD,
                settle_hours,
                range: HistoryRange::unbounded(),
                deferred_only: matches!(mode, CompactMode::DeferredOnly),
            },
            task,
        )
        .await?;
    Ok(())
}

// -- Snapshot formatting ------------------------------------------------------

fn snapshot_header() -> String {
    format!(
        "{:<10} {:<14} {:<14} {:<8} {}",
        "step", "frames", "history", "deferred", "kept rows"
    )
}

/// One line per step: the frame types, the history checkpoints, and the rows
/// actually stored, all indexed by graph ID 1..=E2E_FRAMES.
///
/// ```text
/// frames   F Full   D Delta   X Error   . Empty (unbuilt)
/// history  P Processed   ! Processed-but-deferred   o Omitted
///          e Empty stamp   X Error   . no checkpoint yet
/// kept     N a stored sample   N* an anchor for the sample after it
/// ```
async fn timeline_state(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<TimelineState> {
    let frames = db
        .frames
        .select(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                with_data: Some(false),
                order: Some(Order::Asc),
                ..Default::default()
            },
            task,
        )
        .await?
        .into_iter()
        .map(|row| (row.frame.graph_id, row.frame_type))
        .collect::<BTreeMap<_, _>>();

    let mut conn = db.graph_conn().await?;
    let ids = (1..=E2E_FRAMES).map(GraphID).collect::<Vec<_>>();
    let statuses = conn
        .get_history_status(timeline_id, &ids, task)
        .await?
        .into_iter()
        .map(|row| (row.graph_id, row))
        .collect::<BTreeMap<_, _>>();
    drop(conn);

    let frame_chars = ids
        .iter()
        .map(|id| frames.get(id).map_or('?', frame_type_char))
        .collect::<String>();
    let status_chars = ids
        .iter()
        .map(|id| status_char(statuses.get(id)))
        .collect::<String>();
    let kept = db
        .graph_history
        .series(timeline_id, "app", &UNBOUNDED, task)
        .await?
        .iter()
        .map(|row| match row.anchor {
            true => format!("{}*", row.graph_id.0),
            false => row.graph_id.0.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");

    Ok(TimelineState {
        frames: frame_chars,
        statuses: status_chars,
        kept,
    })
}

/// The three per-graph-ID strings a snapshot row is built from.
struct TimelineState {
    frames: String,
    statuses: String,
    kept: String,
}

/// A row of the per-step table, which also tracks the deferral count.
async fn snapshot_row(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    label: &str,
    deferred: usize,
    task: &ll::Task,
) -> Result<String> {
    let state = timeline_state(db, timeline_id, task).await?;
    Ok(format!(
        "{label:<10} {:<14} {:<14} {deferred:<8} {}",
        state.frames, state.statuses, state.kept
    ))
}

/// A row of the fill-order table, which only compares end states.
async fn fill_order_row(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    label: &str,
    task: &ll::Task,
) -> Result<String> {
    let state = timeline_state(db, timeline_id, task).await?;
    Ok(format!(
        "{label:<28} {:<14} {:<14} {}",
        state.frames, state.statuses, state.kept
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

fn status_char(status: Option<&HistoryStatusRow>) -> char {
    let Some(status) = status else {
        return '.';
    };
    match status.status.parse::<HistoryStatus>() {
        Ok(HistoryStatus::Processed) if status.omission_deferred => '!',
        Ok(HistoryStatus::Processed) => 'P',
        Ok(HistoryStatus::Omitted) => 'o',
        Ok(HistoryStatus::Empty) => 'e',
        Ok(HistoryStatus::Error) => 'X',
        Err(_) => '?',
    }
}

/// A deferred row must not become the baseline the threshold compares against.
///
/// Deferred rows are provisional — compaction may delete them. If one is
/// allowed to advance the baseline, a later frame gets measured against a
/// sample that is about to disappear, and can be *omitted* on that basis.
/// Omission is permanent, so the accumulated drift it was hiding is lost for
/// good and no amount of compaction brings it back.
///
/// Here frame 3 is deferred behind the hole at 2. Measured against frame 3
/// (109), frame 4 (117) looks like a step of 8 and gets dropped; measured
/// against the surviving baseline at frame 1 (100), it is a step of 17 and
/// must be kept.
///
/// Values sit well above zero on purpose: `extract_node_metrics` drops
/// all-zero nodes, so a 0.0 sample would vanish before the threshold ever
/// sees it and muddy what this test is measuring.
#[tokio::test]
async fn graph_history_deferred_rows_do_not_shift_the_threshold_baseline() -> Result<()> {
    let task = ll::Task::create_new("test");
    let values = [100.0f32, 101.0, 109.0, 117.0];
    let threshold = 10.0;

    let db = make_db();
    let timeline_id = setup_timeline(&db, "history_baseline", &task).await?;
    register_empty_frames(&db, &timeline_id, &[1, 2, 3, 4], &task).await?;

    // Frame 2 is still unbuilt, so frames 3 and 4 are judged across the hole.
    fill_frame(&db, &timeline_id, 1, values[0], &task).await?;
    fill_frame(&db, &timeline_id, 3, values[2], &task).await?;
    fill_frame(&db, &timeline_id, 4, values[3], &task).await?;
    db.graph_history
        .ingest(&timeline_id, &deferred_ingest_opts(threshold), &task)
        .await?;

    // The hole closes and everything settles.
    fill_frame(&db, &timeline_id, 2, values[1], &task).await?;
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, threshold), &task)
        .await?;
    db.graph_history
        .compact(&timeline_id, &compact_opts(threshold), &task)
        .await?;

    // Frames 1 and 4 are the samples; 3 rides along as frame 4's anchor.
    let oracle =
        in_order_kept_ids_for(&values, threshold, "history_baseline_oracle", &task).await?;
    k9::snapshot!(format!("{oracle:?}"), "[1, 3, 4]");
    assert_eq!(
        kept_graph_ids(&db, &timeline_id, &task).await?,
        oracle,
        "frame 4's drift from the surviving baseline must survive the deferral"
    );
    Ok(())
}

/// The oracle for an arbitrary value series: ingested in order, nothing ever
/// unsettled, so the threshold chain is the plain one.
async fn in_order_kept_ids_for(
    values: &[f32],
    threshold: f64,
    timeline_name: &str,
    task: &ll::Task,
) -> Result<Vec<i64>> {
    let db = make_db();
    let timeline_id = setup_timeline(&db, timeline_name, task).await?;
    for (index, value) in values.iter().enumerate() {
        fill_frame(&db, &timeline_id, i64::try_from(index)? + 1, *value, task).await?;
    }
    db.graph_history
        .ingest(&timeline_id, &ingest_opts(1, threshold), task)
        .await?;
    kept_graph_ids(&db, &timeline_id, task).await
}

/// Every fill ordering of the same frames must land on the same series.
///
/// The wave pattern in the test above is one shape out of many, and a single
/// shape can miss a real defect purely by luck — the first version of this
/// suite passed against a build that provably lost a sample. So sweep several
/// orderings, each against its own in-order oracle: sequential (the happy
/// path), workers finishing back-to-front, round-robin interleaving, and a
/// pathological one-frame-per-wave descent.
#[tokio::test]
async fn graph_history_any_fill_order_converges_on_the_in_order_result() -> Result<()> {
    let task = ll::Task::create_new("test");
    let values = random_walk(usize::try_from(E2E_FRAMES)?);

    let scenarios: Vec<(&str, Vec<Vec<i64>>)> = vec![
        (
            "sequential",
            vec![vec![1, 2, 3, 4, 5, 6], vec![8, 9, 10], vec![11, 12, 13, 14]],
        ),
        (
            "back-to-front",
            vec![
                vec![12, 13, 14],
                vec![8, 9, 10],
                vec![4, 5, 6],
                vec![1, 2, 3],
                vec![11],
            ],
        ),
        (
            "interleaved",
            vec![vec![1, 4, 10, 13], vec![2, 5, 8, 11, 14], vec![3, 6, 9, 12]],
        ),
        (
            "one at a time",
            (1..=E2E_FRAMES)
                .rev()
                .filter(|id| *id != 7)
                .map(|id| vec![id])
                .collect(),
        ),
    ];

    let mut table = vec![format!(
        "{:<28} {:<14} {:<14} {}",
        "fill order / compaction", "frames", "history", "kept rows"
    )];
    for (name, waves) in &scenarios {
        let waves = waves.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let oracle = in_order_kept_ids(
            &built_ids(&waves),
            &values,
            &format!("history_order_{name}_oracle"),
            &task,
        )
        .await?;

        for (mode, mode_name) in [
            (CompactMode::DeferredOnly, "deferred"),
            (CompactMode::EveryNode, "every node"),
        ] {
            let db = make_db();
            let timeline_id =
                setup_timeline(&db, &format!("history_order_{name}_{mode_name}"), &task).await?;
            run_fill_order(&db, &timeline_id, &waves, &values, mode, &task).await?;

            table.push(
                fill_order_row(&db, &timeline_id, &format!("{name}/{mode_name}"), &task).await?,
            );
            assert_eq!(
                kept_graph_ids(&db, &timeline_id, &task).await?,
                oracle,
                "'{name}' fill order compacted per {mode_name} must converge on the in-order result"
            );
        }
    }

    k9::snapshot!(
        table.join("\n"),
        "
fill order / compaction      frames         history        kept rows
sequential/deferred          FFFFFF.FFFFFFF PoooooePPPPPPP 1,9*,10
sequential/every node        FFFFFF.FFFFFFF PoooooePPPPPPP 1,9*,10
back-to-front/deferred       FFFFFF.FFFFFFF PooPPPePPPPPPP 1,9*,10
back-to-front/every node     FFFFFF.FFFFFFF PooPPPePPPPPPP 1,9*,10
interleaved/deferred         FFFFFF.FFFFFFF PooPPPePPPPPPP 1,9*,10
interleaved/every node       FFFFFF.FFFFFFF PooPPPePPPPPPP 1,9*,10
one at a time/deferred       FFFFFF.FFFFFFF PPPPPPePPPPPPP 1,9*,10
one at a time/every node     FFFFFF.FFFFFFF PPPPPPePPPPPPP 1,9*,10
"
    );
    Ok(())
}

/// Every graph ID that some wave fills, ascending.
fn built_ids(waves: &[&[i64]]) -> Vec<i64> {
    let mut ids = waves
        .iter()
        .flat_map(|wave| wave.iter().copied())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

// -- Delta chains -------------------------------------------------------------

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
    let values = random_walk(20);

    // A single stored range: one Full followed by 19 Deltas.
    let chained = make_db();
    let chained_id = setup_timeline(&chained, "history_delta_chain", &task).await?;
    register_empty_frames(&chained, &chained_id, &(1..=20).collect::<Vec<_>>(), &task).await?;

    let mut builder = GraphRangeBuilder::new(chained_id.clone());
    for (index, value) in values.iter().enumerate() {
        let graph_id = i64::try_from(index)? + 1;
        builder.add(
            GraphTimeKey {
                timeline_id: chained_id.clone(),
                graph_id: GraphID(graph_id),
                timestamp: recent_timestamp(u64::try_from(graph_id)?)?,
            },
            one_node_graph(&[("size", *value)]),
        )?;
    }
    chained
        .graph
        .adjacent_deltas
        .store_range(builder.finalize(), &task)
        .await?;

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

    chained
        .graph_history
        .ingest(&chained_id, &ingest_opts(1, threshold), &task)
        .await?;

    assert_eq!(
        kept_graph_ids(&chained, &chained_id, &task).await?,
        in_order_kept_ids_for(&values, threshold, "history_delta_chain_oracle", &task).await?,
        "replaying a delta chain must produce the same series as standalone Full frames"
    );
    Ok(())
}

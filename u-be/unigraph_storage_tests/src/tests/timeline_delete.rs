// Copyright (c) Meta Platforms, Inc. and affiliates.

//! End-to-end tests for `db.timelines.delete`, against the real SQLite backend.
//!
//! The three things worth proving:
//!
//! - **Everything goes.** Frames, recorded history, metric history, external ID
//!   mappings, the config row — and the external blobs, which are the only part
//!   that leaves through a second door (the cleanup table, then the sweep).
//! - **Only that timeline goes.** Every statement is scoped by `timeline_id`,
//!   and a sibling sharing the database must come out untouched.
//! - **The batch size is only a batch size.** However small, the pass covers
//!   the whole timeline; the only thing that changes is how many transactions
//!   it takes.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use unigraph_db::HistoryIngestOptions;
use unigraph_db::TimelineDeleteOptions;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;

use crate::*;

// ── Fixtures ─────────────────────────────────────────────────

fn make_db() -> (UnigraphDb, Arc<SqliteStorage>) {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    (UnigraphDb::new(sqlite.clone(), sqlite.clone()), sqlite)
}

async fn setup_timeline(
    db: &UnigraphDb,
    name: &str,
    blob_storage: BlobStorageMode,
    external_id_namespace: Option<&str>,
    task: &ll::Task,
) -> Result<TimelineID> {
    let timeline_id = TimelineID(name.to_string());
    db.timelines
        .create(
            &timeline_id,
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: external_id_namespace
                    .map(|ns| ExternalIDNamespace(ns.to_string())),
                blob_storage,
                store_metric_history: Some(true),
            },
            task,
        )
        .await?;
    Ok(timeline_id)
}

async fn store_graphs(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    count: i64,
    task: &ll::Task,
) -> Result<()> {
    for id in 1..=count {
        let key = make_graph_time_key(&timeline_id.0, id, 1000 + id);
        db.graph
            .store(&key, &TestGraphTimeline::get_nth(id as u64), None, task)
            .await?;
    }
    Ok(())
}

/// Everything the timeline still owns, as counts. What "deleted" has to zero.
async fn remains(db: &UnigraphDb, timeline_id: &TimelineID, task: &ll::Task) -> Result<Vec<usize>> {
    let frames = db.frames.list(timeline_id, task).await?.len();
    let config = usize::from(db.timelines.get_config(timeline_id, task).await?.is_some());
    let listed = usize::from(db.timelines.list(task).await?.contains(timeline_id));
    Ok(vec![frames, config, listed])
}

fn delete_now() -> TimelineDeleteOptions {
    TimelineDeleteOptions {
        batch_size: 2,
        // No concurrent writers in a test, so the deferral window that keeps a
        // sweep from racing an in-flight store buys nothing here.
        sweep_min_age: Duration::ZERO,
    }
}

// ── Tests ────────────────────────────────────────────────────

/// The payload flags are what make the batched delete affordable, so pin all
/// three modes — including the one thing `with_manifest` must still answer
/// without reading a payload: whether the frame owns external blobs.
#[tokio::test]
async fn a_query_reads_exactly_the_payload_columns_it_asked_for() -> Result<()> {
    let cases = [
        // (blob_storage, expected blobs_are_inline, why)
        (
            BlobStorageMode::Inline,
            true,
            "small test graphs stay in the inline_blobs column",
        ),
        (
            BlobStorageMode::External,
            false,
            "External mode always uploads, leaving the column NULL",
        ),
    ];

    for (blob_storage, expect_inline, why) in cases {
        let (db, _sqlite) = make_db();
        let task = ll::Task::create_new("test");
        let timeline_id = setup_timeline(&db, "test", blob_storage, None, &task).await?;
        store_graphs(&db, &timeline_id, 1, &task).await?;

        let read = async |with_manifest: bool, with_data: bool| -> Result<FrameRow> {
            let mut rows = db
                .frames
                .select(
                    &FrameQuery {
                        timeline_id: timeline_id.clone(),
                        with_manifest: Some(with_manifest),
                        with_data: Some(with_data),
                        limit: Some(1),
                        frame_types: None,
                        order: None,
                        timestamp_bounds: None,
                        graph_id_bounds: None,
                        graph_ids: None,
                        before: None,
                        expires_before: None,
                    },
                    &task,
                )
                .await?;
            rows.pop().context("the fixture frame should exist")
        };

        let metadata = read(false, false).await?;
        assert!(
            metadata.manifest_json.is_none()
                && metadata.inline_blobs.is_none()
                && metadata.blobs_are_inline.is_none(),
            "a metadata read must touch no payload column ({why})"
        );

        let manifest = read(true, false).await?;
        assert!(
            manifest.manifest_json.is_some(),
            "with_manifest must return the manifest ({why})"
        );
        assert!(
            manifest.inline_blobs.is_none(),
            "with_manifest must not read the payload — that is the whole point ({why})"
        );
        assert_eq!(
            manifest.blobs_are_inline,
            Some(expect_inline),
            "with_manifest must still answer where the blobs live, without \
             reading them: {why}"
        );

        let data = read(false, true).await?;
        assert!(
            data.manifest_json.is_some(),
            "with_data implies with_manifest ({why})"
        );
        assert_eq!(
            data.inline_blobs.is_some(),
            expect_inline,
            "with_data returns inline blobs exactly when there are some ({why})"
        );
        assert_eq!(
            data.blobs_are_inline,
            Some(expect_inline),
            "with_data agrees with with_manifest about where the blobs live ({why})"
        );
    }

    Ok(())
}

#[tokio::test]
async fn deletes_everything_the_timeline_owns_and_nothing_else() -> Result<()> {
    let (db, sqlite) = make_db();
    let task = ll::Task::create_new("test");

    let doomed = setup_timeline(
        &db,
        "doomed",
        BlobStorageMode::External,
        Some("doomed/git"),
        &task,
    )
    .await?;
    let survivor = setup_timeline(
        &db,
        "survivor",
        BlobStorageMode::External,
        Some("survivor/git"),
        &task,
    )
    .await?;

    store_graphs(&db, &doomed, 5, &task).await?;
    store_graphs(&db, &survivor, 3, &task).await?;
    db.graph_history
        .ingest(
            &doomed,
            &HistoryIngestOptions {
                lookback_hours: None,
                threshold: 0.0,
                graph_id_bounds: (None, None),
            },
            &task,
        )
        .await?;
    db.external_ids
        .add_new(
            &ExternalIDNamespace("doomed/git".to_string()),
            &[ExternalID("aaa".to_string()), ExternalID("bbb".to_string())],
            &task,
        )
        .await?;

    // History has to have recorded something, or "history is gone" proves nothing.
    let history_before = db
        .graph_history
        .series(&doomed, "n_000", &TimestampBounds::default(), &task)
        .await?;
    assert!(
        !history_before.is_empty(),
        "the fixture should have recorded history to delete"
    );

    let report = db.timelines.delete(&doomed, &delete_now(), &task).await?;

    assert_eq!(report.frames_deleted, 5, "every frame should be deleted");
    assert_eq!(
        report.frame_batches, 3,
        "5 frames at a batch size of 2 is 3 transactions"
    );
    assert_eq!(
        report.external_ids_deleted, 2,
        "the namespace is this timeline's alone, so its mappings go too"
    );
    assert!(
        report.history.entries_deleted > 0,
        "recorded history should have been deleted: {report:?}"
    );
    assert!(
        report.metric_history_deleted > 0,
        "weekly metric history should have been deleted: {report:?}"
    );

    assert_eq!(
        remains(&db, &doomed, &task).await?,
        vec![0, 0, 0],
        "no frames, no config row, and gone from the timeline list"
    );
    assert!(
        db.graph_history
            .series(&doomed, "n_000", &TimestampBounds::default(), &task)
            .await?
            .is_empty(),
        "history should be gone"
    );
    assert!(
        db.external_ids
            .get_latest(&ExternalIDNamespace("doomed/git".to_string()), &task)
            .await?
            .is_none(),
        "external ID mappings should be gone"
    );

    // Blobs leave through the cleanup table, so both ends have to be clear.
    assert_eq!(
        sqlite.list_blobs("graphs/doomed/").await?,
        Vec::<String>::new(),
        "the deleted timeline's blobs should be physically gone"
    );
    assert_eq!(
        db.blob_storage.get_pending_cleanup(&task).await?,
        Vec::<String>::new(),
        "the sweep should have drained the cleanup table it filled"
    );
    assert!(
        report.blobs_registered > 0 && report.blobs_swept > 0,
        "the report should account for the blobs it removed: {report:?}"
    );

    assert_eq!(
        remains(&db, &survivor, &task).await?,
        vec![3, 1, 1],
        "the sibling timeline should be untouched"
    );
    assert!(
        !sqlite.list_blobs("graphs/survivor/").await?.is_empty(),
        "the sibling's blobs should survive the sweep"
    );

    Ok(())
}

/// The batch size changes how many transactions it takes and nothing else.
#[tokio::test]
async fn every_batch_size_deletes_the_whole_timeline() -> Result<()> {
    let cases = [
        // (batch_size, expected_batches, why)
        (1, 6, "one frame per transaction"),
        (
            4,
            2,
            "6 frames in batches of 4 is a full batch and a partial one",
        ),
        (
            6,
            1,
            "a batch size that fits the timeline exactly takes one pass",
        ),
        (100, 1, "an oversized batch is not an error, just one pass"),
    ];

    for (batch_size, expected_batches, why) in cases {
        let (db, _sqlite) = make_db();
        let task = ll::Task::create_new("test");
        let timeline_id =
            setup_timeline(&db, "test", BlobStorageMode::External, None, &task).await?;
        store_graphs(&db, &timeline_id, 6, &task).await?;

        let report = db
            .timelines
            .delete(
                &timeline_id,
                &TimelineDeleteOptions {
                    batch_size,
                    sweep_min_age: Duration::ZERO,
                },
                &task,
            )
            .await?;

        assert_eq!(report.frames_deleted, 6, "{why} (batch_size={batch_size})");
        assert_eq!(
            report.frame_batches, expected_batches,
            "{why} (batch_size={batch_size})"
        );
        assert_eq!(
            remains(&db, &timeline_id, &task).await?,
            vec![0, 0, 0],
            "{why} (batch_size={batch_size})"
        );
    }

    Ok(())
}

/// A namespace two timelines declare is the ID allocator's state, not this
/// timeline's data. Dropping it would reset the survivor's `GraphID` sequence
/// to zero and hand out IDs it has already used.
#[tokio::test]
async fn a_shared_external_id_namespace_is_left_alone() -> Result<()> {
    let (db, _sqlite) = make_db();
    let task = ll::Task::create_new("test");

    let namespace = ExternalIDNamespace("shared/git".to_string());
    let doomed = setup_timeline(
        &db,
        "doomed",
        BlobStorageMode::Inline,
        Some("shared/git"),
        &task,
    )
    .await?;
    setup_timeline(
        &db,
        "survivor",
        BlobStorageMode::Inline,
        Some("shared/git"),
        &task,
    )
    .await?;
    db.external_ids
        .add_new(&namespace, &[ExternalID("aaa".to_string())], &task)
        .await?;

    let report = db.timelines.delete(&doomed, &delete_now(), &task).await?;

    assert_eq!(report.external_ids_deleted, 0, "nothing should be deleted");
    assert_eq!(
        report.external_id_namespace_shared_with,
        Some(TimelineID("survivor".to_string())),
        "the report must name who still needs the namespace"
    );
    assert_eq!(
        db.external_ids.get_latest(&namespace, &task).await?,
        Some(ExternalID("aaa".to_string())),
        "the survivor's mappings should still be there"
    );

    Ok(())
}

/// A registered key whose blob was never written must not wedge the sweep.
///
/// This is a normal state, not corruption: the crash-safe store path registers
/// a blob key *before* uploading it and unregisters only on commit, so any
/// store that dies in between leaves a registration behind forever. Since
/// `sweep_blobs` unregisters its batch only if every delete succeeded, a
/// backend that errors on a missing key stops all cleanup, for every timeline —
/// which is exactly what a Manifold `[404]` used to do.
#[tokio::test]
async fn a_stale_cleanup_registration_does_not_wedge_the_sweep() -> Result<()> {
    let (db, sqlite) = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "test", BlobStorageMode::External, None, &task).await?;
    store_graphs(&db, &timeline_id, 2, &task).await?;

    // A key for a blob that was never uploaded, as a half-finished store leaves.
    let mut conn = db.graph_conn_write().await?;
    conn.register_blobs_for_cleanup(&["graphs/test/999/never_uploaded".to_owned()], &task)
        .await?;
    drop(conn);

    let report = db
        .timelines
        .delete(&timeline_id, &delete_now(), &task)
        .await?;

    assert!(
        report.blobs_swept > 0,
        "the sweep must get through the batch the stale key is in: {report:?}"
    );
    assert_eq!(
        db.blob_storage.get_pending_cleanup(&task).await?,
        Vec::<String>::new(),
        "the stale key should be unregistered along with the real ones — \
         leaving it queued means retrying it forever"
    );
    assert_eq!(
        sqlite.list_blobs("graphs/test/").await?,
        Vec::<String>::new(),
        "the real blobs should still have been deleted"
    );

    Ok(())
}

/// Empty is a legal state, not a special case — a timeline created and never
/// written to still has a config row to delete.
#[tokio::test]
async fn deleting_an_empty_timeline_works() -> Result<()> {
    let (db, _sqlite) = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "test", BlobStorageMode::Inline, None, &task).await?;

    let report = db
        .timelines
        .delete(&timeline_id, &delete_now(), &task)
        .await?;

    assert_eq!(report.frames_deleted, 0);
    assert_eq!(report.frame_batches, 0, "no frames means no transactions");
    assert_eq!(remains(&db, &timeline_id, &task).await?, vec![0, 0, 0]);

    Ok(())
}

#[tokio::test]
async fn refuses_a_timeline_that_does_not_exist_and_a_nonsense_batch_size() -> Result<()> {
    let (db, _sqlite) = make_db();
    let task = ll::Task::create_new("test");
    let timeline_id = setup_timeline(&db, "test", BlobStorageMode::Inline, None, &task).await?;

    let missing = db
        .timelines
        .delete(&TimelineID("nope".to_string()), &delete_now(), &task)
        .await;
    assert!(
        format!("{:#}", missing.unwrap_err()).contains("Timeline not found"),
        "deleting a timeline that isn't there should say so, not report success"
    );

    let bad_batch = db
        .timelines
        .delete(
            &timeline_id,
            &TimelineDeleteOptions {
                batch_size: 0,
                sweep_min_age: Duration::ZERO,
            },
            &task,
        )
        .await;
    assert!(
        format!("{:#}", bad_batch.unwrap_err()).contains("batch_size must be positive"),
        "a zero batch size would loop forever — it has to be rejected up front"
    );
    assert_eq!(
        remains(&db, &timeline_id, &task).await?,
        vec![0, 1, 1],
        "the rejected delete should not have touched the timeline"
    );

    Ok(())
}

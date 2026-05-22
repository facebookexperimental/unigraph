// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::cmp;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use unigraph_db::GraphRangeBuilder;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::AdjacentDeltasConfig;
use unigraph_storage_core::ExternalID;
use unigraph_storage_core::Frame;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::TimestampedError;

use crate::config::IngestionPipelineConfig;
use crate::config::IngestionSource;
use crate::graph_builder::Builder;

/// Options that control ingestion behavior.
pub struct IngestionOptions {
    /// Maximum number of new commits to ingest. `None` means unlimited.
    pub limit: Option<usize>,
}

/// Run the full ingestion pipeline, dispatching on the source type.
pub async fn run_ingestion(
    config: &IngestionPipelineConfig<'_>,
    db: &UnigraphDb,
    options: &IngestionOptions,
) -> Result<()> {
    let task = ll::Task::create_new("ingestion");

    match &config.source {
        IngestionSource::Git {
            repo_path,
            main_branch,
            ..
        } => run_git_ingestion(config, db, repo_path, main_branch, options, &task).await,
    }
}

// -------------------------------------------------------------------
// Git source ingestion
// -------------------------------------------------------------------

/// Ingest from a git repository.
///
/// **Phase A**: Discovers new commits, allocates GraphIDs, registers empty frames.
/// **Phase B**: For each empty frame, checks out the commit, builds the graph, stores it.
async fn run_git_ingestion(
    config: &IngestionPipelineConfig<'_>,
    db: &UnigraphDb,
    repo_path: &Path,
    _main_branch: &str,
    options: &IngestionOptions,
    task: &ll::Task,
) -> Result<()> {
    let ns = config.source.external_id_namespace();

    // === Phase A: Registration ===

    let registration_task = task.create("discover_commits");

    let latest_external_id = db.external_ids.get_latest(ns, task).await?;
    let since_hash = latest_external_id.map(|eid| eid.0);

    let mut new_commits =
        unigraph_git::collect_linear_history_since(repo_path, since_hash.as_deref())
            .context("Failed to collect git history")?;

    if let Some(limit) = options.limit
        && new_commits.len() > limit
    {
        registration_task.data("limited_from", new_commits.len());
        new_commits.truncate(limit);
    }

    registration_task.data("new_commits", new_commits.len());

    if new_commits.is_empty() {
        return Ok(());
    }

    let external_ids: Vec<ExternalID> = new_commits
        .iter()
        .map(|c| ExternalID(c.hash.clone()))
        .collect();
    let graph_ids = db.external_ids.add_new(ns, &external_ids, task).await?;

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        ensure_timeline(db, timeline_id, ns, task).await?;

        let new_frames: Vec<Frame> = new_commits
            .iter()
            .zip(&graph_ids)
            .map(|(commit, &graph_id)| Frame {
                timestamp: commit.timestamp,
                graph_id,
            })
            .collect();

        db.graph
            .adjacent_deltas
            .put_new_empty_frames(timeline_id, new_frames, true, task)
            .await?;
    }

    drop(registration_task);

    // === Phase B: Processing ===

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        let builder_task = task.create(&format!("build:{}", timeline_id.0));

        let frames = db.frames.list(timeline_id, task).await?;
        let empty_frames: Vec<_> = frames
            .iter()
            .filter(|f| f.frame_type == FrameType::Empty)
            .collect();
        let total = frames.len();
        let to_process = empty_frames.len();
        let already_done = total - to_process;

        builder_task.data("total_frames", total);
        builder_task.data("to_process", to_process);

        let mut stored_count = 0usize;
        let mut error_count = 0usize;

        let Builder::FromRepo(builder) = &builder_config.builder;

        let mut range_builder = GraphRangeBuilder::new(timeline_id.clone());

        for (i, frame) in empty_frames.iter().enumerate() {
            let external_id = db
                .external_ids
                .to_external_id(ns, &frame.frame.graph_id, task)
                .await?
                .context("ExternalID mapping must exist for allocated GraphID")?;
            let commit_hash = &external_id.0;
            let short_hash = &commit_hash[..cmp::min(8, commit_hash.len())];

            let commit_task = builder_task.create(&format!("commit:{short_hash}"));
            builder_task.progress((already_done + i) as i64, total as i64);

            let key = GraphTimeKey {
                timeline_id: timeline_id.clone(),
                timestamp: frame.frame.timestamp,
                graph_id: frame.frame.graph_id,
            };

            // Check out the commit
            if let Err(e) = unigraph_git::checkout_commit(repo_path, commit_hash) {
                // Flush accumulated range before storing error.
                if !range_builder.is_empty() {
                    let flushed = range_builder.take(timeline_id.clone());
                    db.graph.adjacent_deltas.store_range(flushed, task).await?;
                }
                store_error(db, &key, &format!("Failed to checkout: {e:#}"), task).await?;
                commit_task.data("status", "error");
                error_count += 1;
                continue;
            }

            // Build the graph
            match builder.build(repo_path) {
                Ok(map_graph) => match map_graph.to_array_graph_serializable() {
                    Ok(array_graph) => {
                        range_builder.add(key, array_graph)?;
                        commit_task.data("status", "stored");
                        stored_count += 1;
                    }
                    Err(e) => {
                        if !range_builder.is_empty() {
                            let flushed = range_builder.take(timeline_id.clone());
                            db.graph.adjacent_deltas.store_range(flushed, task).await?;
                        }
                        store_error(
                            db,
                            &key,
                            &format!("Failed to convert to ArrayGraphSerializable: {e:#}"),
                            task,
                        )
                        .await?;
                        commit_task.data("status", "error");
                        error_count += 1;
                    }
                },
                Err(e) => {
                    if !range_builder.is_empty() {
                        let flushed = range_builder.take(timeline_id.clone());
                        db.graph.adjacent_deltas.store_range(flushed, task).await?;
                    }
                    store_error(db, &key, &format!("Graph build failed: {e:#}"), task).await?;
                    commit_task.data("status", "error");
                    error_count += 1;
                }
            }
        }

        // Store remaining range.
        if !range_builder.is_empty() {
            let final_range = range_builder.finalize();
            db.graph
                .adjacent_deltas
                .store_range(final_range, task)
                .await?;
        }

        builder_task.progress(total as i64, total as i64);
        builder_task.data("stored", stored_count);
        builder_task.data("skipped", already_done);
        builder_task.data("errors", error_count);
    }

    Ok(())
}

// -------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------

async fn ensure_timeline(
    db: &UnigraphDb,
    timeline_id: &TimelineID,
    ns: &unigraph_storage_core::ExternalIDNamespace,
    task: &ll::Task,
) -> Result<()> {
    if db.timelines.get_config(timeline_id, task).await?.is_none() {
        let timeline_config = TimelineConfig {
            schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            external_id_namespace: Some(ns.clone()),
            blob_storage: Default::default(),
            store_metric_history: None,
        };
        db.timelines
            .create(timeline_id, &timeline_config, task)
            .await?;
    }
    Ok(())
}

async fn store_error(
    db: &UnigraphDb,
    key: &GraphTimeKey,
    message: &str,
    task: &ll::Task,
) -> Result<()> {
    let errors = vec![TimestampedError {
        timestamp: unigraph_timestamp::Timestamp::now(),
        message: message.to_string(),
    }];
    db.graph.store_error(key, &errors, task).await
}

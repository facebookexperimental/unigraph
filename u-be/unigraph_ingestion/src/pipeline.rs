// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::cmp;
use std::collections::HashSet;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::AdjacentDeltasConfig;
use unigraph_storage_core::ExternalID;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
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
        IngestionSource::AnotherTimeline {
            source_timeline_id, ..
        } => run_timeline_ingestion(config, db, source_timeline_id, &task).await,
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

        let existing_graph_ids: HashSet<GraphID> = db
            .frames
            .list(timeline_id, task)
            .await?
            .iter()
            .map(|f| f.frame.graph_id)
            .collect();

        for (commit, &graph_id) in new_commits.iter().zip(&graph_ids) {
            if !existing_graph_ids.contains(&graph_id) {
                let key = GraphTimeKey {
                    timeline_id: timeline_id.clone(),
                    timestamp: commit.timestamp,
                    graph_id,
                };
                db.frames.store_empty(&key, task).await?;
            }
        }
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

        let Builder::FromRepo(builder) = &builder_config.builder else {
            anyhow::bail!(
                "Git source requires FromRepo builders, got BudgetGraph for timeline '{}'",
                timeline_id.0
            );
        };

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
                store_error(db, &key, &format!("Failed to checkout: {e:#}"), task).await?;
                commit_task.data("status", "error");
                error_count += 1;
                continue;
            }

            // Build the graph
            match builder.build(repo_path) {
                Ok(map_graph) => match map_graph.to_array_graph_serializable() {
                    Ok(array_graph) => {
                        db.graph
                            .store(&key, &array_graph, task)
                            .await
                            .with_context(|| format!("Failed to store graph for {short_hash}"))?;
                        commit_task.data("status", "stored");
                        stored_count += 1;
                    }
                    Err(e) => {
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
                    store_error(db, &key, &format!("Graph build failed: {e:#}"), task).await?;
                    commit_task.data("status", "error");
                    error_count += 1;
                }
            }
        }

        builder_task.progress(total as i64, total as i64);
        builder_task.data("stored", stored_count);
        builder_task.data("skipped", already_done);
        builder_task.data("errors", error_count);
    }

    Ok(())
}

// -------------------------------------------------------------------
// AnotherTimeline source ingestion
// -------------------------------------------------------------------

/// Ingest by transforming graphs from an existing timeline.
///
/// **Phase A**: Discovers frames from the source timeline, registers empty frames
/// in the derived timeline(s) using the same GraphIDs and timestamps.
/// **Phase B**: For each empty frame, fetches the source graph and runs
/// the budget builder to produce a derived graph.
async fn run_timeline_ingestion(
    config: &IngestionPipelineConfig<'_>,
    db: &UnigraphDb,
    source_timeline_id: &TimelineID,
    task: &ll::Task,
) -> Result<()> {
    let ns = config.source.external_id_namespace();

    // === Phase A: Registration ===

    let registration_task = task.create("discover_frames");

    let source_frames = db.frames.list(source_timeline_id, task).await?;

    registration_task.data("source_timeline", &source_timeline_id.0);
    registration_task.data("source_frames", source_frames.len());

    if source_frames.is_empty() {
        return Ok(());
    }

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        ensure_timeline(db, timeline_id, ns, task).await?;

        let existing_graph_ids: HashSet<GraphID> = db
            .frames
            .list(timeline_id, task)
            .await?
            .iter()
            .map(|f| f.frame.graph_id)
            .collect();

        for source_frame in &source_frames {
            let graph_id = source_frame.frame.graph_id;
            if !existing_graph_ids.contains(&graph_id) {
                let key = GraphTimeKey {
                    timeline_id: timeline_id.clone(),
                    timestamp: source_frame.frame.timestamp,
                    graph_id,
                };
                db.frames.store_empty(&key, task).await?;
            }
        }
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

        let Builder::BudgetGraph(budget_builder) = &builder_config.builder else {
            anyhow::bail!(
                "AnotherTimeline source requires BudgetGraph builders, \
                 got FromRepo for timeline '{}'",
                timeline_id.0
            );
        };

        for (i, frame) in empty_frames.iter().enumerate() {
            let graph_id = frame.frame.graph_id;

            // Resolve external ID for display
            let display_id = match db.external_ids.to_external_id(ns, &graph_id, task).await? {
                Some(eid) => {
                    let hash = &eid.0;
                    hash[..cmp::min(8, hash.len())].to_string()
                }
                None => format!("g:{}", graph_id.0),
            };

            let frame_task = builder_task.create(&format!("frame:{display_id}"));
            builder_task.progress((already_done + i) as i64, total as i64);

            let key = GraphTimeKey {
                timeline_id: timeline_id.clone(),
                timestamp: frame.frame.timestamp,
                graph_id,
            };

            let source_key = GraphKey {
                timeline_id: source_timeline_id.clone(),
                graph_id,
            };

            // Check if the source frame has data
            let source_frame = db.frames.get(&source_key, false, task).await?;
            match &source_frame {
                Some(sf)
                    if sf.frame_type == FrameType::Full || sf.frame_type == FrameType::Delta =>
                {
                    // Source has data, proceed below
                }
                Some(sf) => {
                    store_error(
                        db,
                        &key,
                        &format!(
                            "Source frame in timeline '{}' is {:?}, skipping",
                            source_timeline_id.0, sf.frame_type
                        ),
                        task,
                    )
                    .await?;
                    frame_task.data("status", "error");
                    error_count += 1;
                    continue;
                }
                None => {
                    store_error(
                        db,
                        &key,
                        &format!(
                            "Source frame not found in timeline '{}'",
                            source_timeline_id.0
                        ),
                        task,
                    )
                    .await?;
                    frame_task.data("status", "error");
                    error_count += 1;
                    continue;
                }
            }

            // Fetch the source graph
            let source_graph = match db.graph.fetch(&source_key, task).await {
                Ok(g) => g,
                Err(e) => {
                    store_error(
                        db,
                        &key,
                        &format!("Failed to fetch source graph: {e:#}"),
                        task,
                    )
                    .await?;
                    frame_task.data("status", "error");
                    error_count += 1;
                    continue;
                }
            };

            // Transform
            match budget_builder.build(source_graph) {
                Ok(result_graph) => {
                    db.graph
                        .store(&key, &result_graph, task)
                        .await
                        .with_context(|| format!("Failed to store graph for {display_id}"))?;
                    frame_task.data("status", "stored");
                    stored_count += 1;
                }
                Err(e) => {
                    store_error(db, &key, &format!("Transform failed: {e:#}"), task).await?;
                    frame_task.data("status", "error");
                    error_count += 1;
                }
            }
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

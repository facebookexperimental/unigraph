// Copyright (c) Meta Platforms, Inc. and affiliates.

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
use crate::progress::IngestionProgress;

/// Run the full ingestion pipeline, dispatching on the source type.
pub async fn run_ingestion(config: &IngestionPipelineConfig<'_>, db: &UnigraphDb) -> Result<()> {
    match &config.source {
        IngestionSource::Git {
            repo_path,
            main_branch,
            ..
        } => run_git_ingestion(config, db, repo_path, main_branch).await,
        IngestionSource::AnotherTimeline {
            source_timeline_id, ..
        } => run_timeline_ingestion(config, db, source_timeline_id).await,
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
) -> Result<()> {
    let ns = config.source.external_id_namespace();

    // === Phase A: Registration ===

    let latest_external_id = db.get_latest_external_id(ns).await?;
    let since_hash = latest_external_id.map(|eid| eid.0);

    eprintln!("Collecting git history from {}...", repo_path.display());
    let new_commits = unigraph_git::collect_linear_history_since(repo_path, since_hash.as_deref())
        .context("Failed to collect git history")?;

    if new_commits.is_empty() {
        eprintln!("No new commits, nothing to do.");
        return Ok(());
    }
    eprintln!("Found {} new commits", new_commits.len());

    let external_ids: Vec<ExternalID> = new_commits
        .iter()
        .map(|c| ExternalID(c.hash.clone()))
        .collect();
    let graph_ids = db.add_new_external_ids(ns, &external_ids).await?;

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        ensure_timeline(db, timeline_id, ns).await?;

        let existing_graph_ids: HashSet<GraphID> = db
            .list_frames(timeline_id)
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
                db.store_frame_empty(&key).await?;
            }
        }
    }

    // === Phase B: Processing ===

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        let frames = db.list_frames(timeline_id).await?;
        let empty_frames: Vec<_> = frames
            .iter()
            .filter(|f| f.frame_type == FrameType::Empty)
            .collect();
        let total = frames.len();
        let to_process = empty_frames.len();
        let already_done = total - to_process;

        eprintln!(
            "[{}] {} total frames, {} already ingested, {} to process",
            timeline_id.0, total, already_done, to_process
        );

        let mut progress = IngestionProgress::new(total);
        let mut stored_count = 0usize;
        let skipped_count = already_done;
        let mut error_count = 0usize;

        for _ in 0..already_done {
            progress.skip_silent();
        }

        let Builder::FromRepo(builder) = &builder_config.builder else {
            anyhow::bail!(
                "Git source requires FromRepo builders, got BudgetGraph for timeline '{}'",
                timeline_id.0
            );
        };

        for frame in &empty_frames {
            let external_id = db
                .graph_id_to_external_id(ns, &frame.frame.graph_id)
                .await?
                .context("ExternalID mapping must exist for allocated GraphID")?;
            let commit_hash = &external_id.0;
            progress.start(commit_hash, "");

            let key = GraphTimeKey {
                timeline_id: timeline_id.clone(),
                timestamp: frame.frame.timestamp,
                graph_id: frame.frame.graph_id,
            };

            // Check out the commit
            if let Err(e) = unigraph_git::checkout_commit(repo_path, commit_hash) {
                store_error(db, &key, &format!("Failed to checkout: {e:#}")).await?;
                progress.error(&e);
                error_count += 1;
                continue;
            }

            // Build the graph
            match builder.build(repo_path) {
                Ok(map_graph) => match map_graph.to_array_graph_serializable() {
                    Ok(array_graph) => {
                        db.store_graph_full(&key, &array_graph)
                            .await
                            .with_context(|| {
                                format!("Failed to store graph for {}", &commit_hash[..8])
                            })?;
                        stored_count += 1;
                    }
                    Err(e) => {
                        store_error(
                            db,
                            &key,
                            &format!("Failed to convert to ArrayGraphSerializable: {e:#}"),
                        )
                        .await?;
                        progress.error(&e);
                        error_count += 1;
                    }
                },
                Err(e) => {
                    store_error(db, &key, &format!("Graph build failed: {e:#}")).await?;
                    progress.error(&e);
                    error_count += 1;
                }
            }
        }

        progress.done(stored_count, skipped_count, error_count);
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
) -> Result<()> {
    let ns = config.source.external_id_namespace();

    // === Phase A: Registration ===

    let source_frames = db.list_frames(source_timeline_id).await?;

    if source_frames.is_empty() {
        eprintln!(
            "Source timeline '{}' has no frames, nothing to do.",
            source_timeline_id.0
        );
        return Ok(());
    }
    eprintln!(
        "Source timeline '{}' has {} frames",
        source_timeline_id.0,
        source_frames.len()
    );

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        ensure_timeline(db, timeline_id, ns).await?;

        let existing_graph_ids: HashSet<GraphID> = db
            .list_frames(timeline_id)
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
                db.store_frame_empty(&key).await?;
            }
        }
    }

    // === Phase B: Processing ===

    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;
        let frames = db.list_frames(timeline_id).await?;
        let empty_frames: Vec<_> = frames
            .iter()
            .filter(|f| f.frame_type == FrameType::Empty)
            .collect();
        let total = frames.len();
        let to_process = empty_frames.len();
        let already_done = total - to_process;

        eprintln!(
            "[{}] {} total frames, {} already ingested, {} to process",
            timeline_id.0, total, already_done, to_process
        );

        let mut progress = IngestionProgress::new(total);
        let mut stored_count = 0usize;
        let skipped_count = already_done;
        let mut error_count = 0usize;

        for _ in 0..already_done {
            progress.skip_silent();
        }

        let Builder::BudgetGraph(budget_builder) = &builder_config.builder else {
            anyhow::bail!(
                "AnotherTimeline source requires BudgetGraph builders, \
                 got FromRepo for timeline '{}'",
                timeline_id.0
            );
        };

        for frame in &empty_frames {
            let graph_id = frame.frame.graph_id;

            // Resolve external ID for progress display
            let display_id = match db.graph_id_to_external_id(ns, &graph_id).await? {
                Some(eid) => eid.0,
                None => format!("graph_id:{}", graph_id.0),
            };
            progress.start(&display_id, "");

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
            let source_frame = db.get_frame(&source_key, false).await?;
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
                    )
                    .await?;
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
                    )
                    .await?;
                    error_count += 1;
                    continue;
                }
            }

            // Fetch the source graph
            let source_graph = match db.fetch_graph(&source_key).await {
                Ok(g) => g,
                Err(e) => {
                    store_error(db, &key, &format!("Failed to fetch source graph: {e:#}")).await?;
                    progress.error(&e);
                    error_count += 1;
                    continue;
                }
            };

            // Transform
            match budget_builder.build(source_graph) {
                Ok(result_graph) => {
                    db.store_graph_full(&key, &result_graph)
                        .await
                        .with_context(|| format!("Failed to store graph for {display_id}"))?;
                    stored_count += 1;
                }
                Err(e) => {
                    store_error(db, &key, &format!("Transform failed: {e:#}")).await?;
                    progress.error(&e);
                    error_count += 1;
                }
            }
        }

        progress.done(stored_count, skipped_count, error_count);
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
) -> Result<()> {
    if db.get_timeline_config(timeline_id).await?.is_none() {
        let timeline_config = TimelineConfig {
            schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
            external_id_namespace: Some(ns.clone()),
            blob_storage: Default::default(),
        };
        db.create_timeline(timeline_id, &timeline_config).await?;
        eprintln!("Created timeline '{}'", timeline_id.0);
    }
    Ok(())
}

async fn store_error(db: &UnigraphDb, key: &GraphTimeKey, message: &str) -> Result<()> {
    let errors = vec![TimestampedError {
        timestamp: unigraph_timestamp::Timestamp::now(),
        message: message.to_string(),
    }];
    db.store_error(key, &errors).await
}

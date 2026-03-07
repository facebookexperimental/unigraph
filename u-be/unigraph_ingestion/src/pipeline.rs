// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashSet;

use anyhow::Context;
use anyhow::Result;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::AdjacentDeltasConfig;
use unigraph_storage_core::ExternalID;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::TimestampedError;

use crate::config::IngestionPipelineConfig;
use crate::config::IngestionSource;
use crate::progress::IngestionProgress;

/// Run the full ingestion pipeline.
///
/// **Phase A** (registration): Discovers new commits from git, allocates
/// sequential GraphIDs (with internal lock + transaction), and registers
/// empty frames for each builder/timeline pair.
///
/// **Phase B** (processing): Iterates over empty frames, checks out the
/// corresponding commit (resolved via the external ID mapping), builds the
/// graph, and stores it. Each `store_graph_full` / `store_error` call manages
/// its own connection and transaction internally.
pub async fn run_ingestion(config: &IngestionPipelineConfig<'_>, db: &UnigraphDb) -> Result<()> {
    let IngestionSource::Git { ref repo_path, .. } = config.source;

    // === Phase A: Registration ===

    // 1. Find the latest known commit in this namespace
    let latest_external_id = db
        .get_latest_external_id(&config.external_id_namespace)
        .await?;
    let since_hash = latest_external_id.map(|eid| eid.0);

    // 2. Get only new commits from git
    eprintln!("Collecting git history from {}...", repo_path.display());
    let new_commits = unigraph_git::collect_linear_history_since(repo_path, since_hash.as_deref())
        .context("Failed to collect git history")?;

    if new_commits.is_empty() {
        eprintln!("No new commits, nothing to do.");
        return Ok(());
    }
    eprintln!("Found {} new commits", new_commits.len());

    // 3. Batch-allocate GraphIDs for new commits.
    //    add_new_external_ids manages its own lock + transaction internally,
    //    handles race conditions if another job inserted IDs concurrently.
    let external_ids: Vec<ExternalID> = new_commits
        .iter()
        .map(|c| ExternalID(c.hash.clone()))
        .collect();
    let graph_ids = db
        .add_new_external_ids(&config.external_id_namespace, &external_ids)
        .await?;

    // 4. For each builder/timeline, create timeline + register empty frames
    for builder_config in &config.builders {
        let timeline_id = &builder_config.timeline_id;

        // Create or verify the timeline exists
        if db.get_timeline_config(timeline_id).await?.is_none() {
            let timeline_config = TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: Some(config.external_id_namespace.clone()),
                blob_storage: Default::default(),
            };
            db.create_timeline(timeline_id, &timeline_config).await?;
            eprintln!("Created timeline '{}'", timeline_id.0);
        }

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

    // === Phase B: Processing (no long-held transaction) ===
    //
    // Each store_graph_full / store_error call manages its own
    // connection + transaction internally via UnigraphStorage.

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

        // Skip already-ingested frames in the progress counter
        for _ in 0..already_done {
            progress.skip_silent();
        }

        for frame in &empty_frames {
            let external_id = db
                .graph_id_to_external_id(&config.external_id_namespace, &frame.frame.graph_id)
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
                let errors = vec![TimestampedError {
                    timestamp: unigraph_timestamp::Timestamp::now(),
                    message: format!("Failed to checkout: {e:#}"),
                }];
                db.store_error(&key, &errors).await?;
                progress.error(&e);
                error_count += 1;
                continue;
            }

            // Build the graph
            match builder_config.builder.build(repo_path) {
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
                        let errors = vec![TimestampedError {
                            timestamp: unigraph_timestamp::Timestamp::now(),
                            message: format!("Failed to convert to ArrayGraphSerializable: {e:#}"),
                        }];
                        db.store_error(&key, &errors).await?;
                        progress.error(&e);
                        error_count += 1;
                    }
                },
                Err(e) => {
                    let errors = vec![TimestampedError {
                        timestamp: unigraph_timestamp::Timestamp::now(),
                        message: format!("Graph build failed: {e:#}"),
                    }];
                    db.store_error(&key, &errors).await?;
                    progress.error(&e);
                    error_count += 1;
                }
            }
        }

        progress.done(stored_count, skipped_count, error_count);
    }

    Ok(())
}

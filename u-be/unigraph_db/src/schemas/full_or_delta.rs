// Copyright (c) Meta Platforms, Inc. and affiliates.

//! FullOrDelta timeline schema — explicit delta storage with no ordering constraints.
//!
//! Unlike AdjacentDeltas, this schema:
//! - Has no monotonic ordering requirements (graphs can be stored in any order)
//! - Allows deltas to reference any graph as a base (including cross-timeline)
//! - Does not support compaction (deltas are created explicitly, not automatically)
//! - Uses an iterative chain walker for fetch (follows base references backward)

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;

use crate::context::UnigraphDbContext;
use crate::storage::UnigraphStorage;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Store a full graph snapshot in a FullOrDelta timeline.
///
/// No ordering validation — graphs can be stored in any order.
pub async fn store_full(
    ctx: &UnigraphDbContext,
    key: &GraphTimeKey,
    graph: &ArrayGraphSerializable,
    task: &ll::Task,
) -> Result<()> {
    let storage = &ctx.storage;
    let prepared_history = storage.prepare_history_if_enabled(key, graph, task).await?;

    let config = ctx.pack_config_for_key(key);
    let package = graph.pack(&config).context("Failed to pack graph")?;
    let manifest_json =
        serde_json::to_string(&package.manifest).context("Failed to serialize graph manifest")?;

    let prepared = storage
        .prepare_blobs_for_storage(&key.timeline_id, &package.blobs, task)
        .await?;

    let mut conn = storage.graph.conn_write().await?;
    conn.start_transaction(task).await?;
    conn.get_timeline_config_and_lock(&key.timeline_id, task)
        .await?;

    storage
        .store_package_on_conn(
            &mut *conn,
            key,
            FrameType::Full,
            None,
            &manifest_json,
            prepared.inline.as_deref(),
            prepared.external_keys.as_deref(),
            None,
            task,
        )
        .await?;

    if let Some(prepared_history) = prepared_history {
        crate::metric_history::store_metric_history_on_conn(
            &mut *conn,
            &key.timeline_id,
            prepared_history,
            task,
        )
        .await?;
    }

    conn.commit_transaction(task).await?;
    Ok(())
}

/// Store a graph as a delta from another graph.
///
/// Fetches the base graph, derives the delta, packs it, and stores.
/// The base can be in any timeline (cross-timeline deltas are allowed).
pub async fn store_delta(
    ctx: &UnigraphDbContext,
    key: &GraphTimeKey,
    base_key: &GraphKey,
    target_graph: &ArrayGraphSerializable,
    task: &ll::Task,
) -> Result<()> {
    let storage = &ctx.storage;
    let prepared_history = storage
        .prepare_history_if_enabled(key, target_graph, task)
        .await?;

    let base_graph = storage
        .fetch_graph(base_key, task)
        .await
        .with_context(|| format!("Failed to fetch base graph {:?}", base_key))?;

    let delta =
        unigraph_core::derive_delta(&base_graph, target_graph).context("Failed to derive delta")?;

    let config = ctx.pack_config_for_key(key);
    let package = unigraph_core::pack_delta(&delta, &config).context("Failed to pack delta")?;
    let manifest_json =
        serde_json::to_string(&package.manifest).context("Failed to serialize delta manifest")?;

    let prepared = storage
        .prepare_blobs_for_storage(&key.timeline_id, &package.blobs, task)
        .await?;

    let mut conn = storage.graph.conn_write().await?;
    conn.start_transaction(task).await?;
    conn.get_timeline_config_and_lock(&key.timeline_id, task)
        .await?;

    storage
        .store_package_on_conn(
            &mut *conn,
            key,
            FrameType::Delta,
            Some(base_key),
            &manifest_json,
            prepared.inline.as_deref(),
            prepared.external_keys.as_deref(),
            None,
            task,
        )
        .await?;

    if let Some(prepared_history) = prepared_history {
        crate::metric_history::store_metric_history_on_conn(
            &mut *conn,
            &key.timeline_id,
            prepared_history,
            task,
        )
        .await?;
    }

    conn.commit_transaction(task).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/// Fetch a graph from a FullOrDelta timeline.
///
/// Walks the delta chain backwards until a Full frame is found, then
/// reconstructs forward by applying deltas in order.
///
/// When the chain crosses into another timeline, delegates to
/// `storage.fetch_graph()` for schema-dispatched fetch of the base,
/// then applies remaining same-timeline deltas.
pub async fn fetch_graph(
    storage: &UnigraphStorage,
    key: &GraphKey,
    task: &ll::Task,
) -> Result<ArrayGraphSerializable> {
    let mut chain: Vec<(GraphKey, ChainEntry)> = Vec::new();
    let mut current_key = key.clone();
    let mut visited = std::collections::HashSet::new();

    // Walk backwards collecting frames until we hit a Full frame or cross timelines.
    loop {
        anyhow::ensure!(
            visited.insert(current_key.clone()),
            "cycle detected in delta chain at {:?}",
            current_key,
        );

        let row = storage.get_frame_with_data(&current_key, task).await?;

        match row.frame_type {
            FrameType::Full => {
                let data = row
                    .data
                    .ok_or_else(|| anyhow::anyhow!("Full frame {:?} has no data", current_key))?;
                chain.push((current_key, ChainEntry::Full(data)));
                break;
            }
            FrameType::Delta => {
                let base = row.base.ok_or_else(|| {
                    anyhow::anyhow!("Delta frame {:?} has no base key", current_key)
                })?;
                let data = row
                    .data
                    .ok_or_else(|| anyhow::anyhow!("Delta frame {:?} has no data", current_key))?;

                // If base is in a different timeline, fetch it via schema dispatch
                // and stop walking.
                if base.timeline_id != key.timeline_id {
                    let base_graph = storage.fetch_graph(&base, task).await.with_context(|| {
                        format!(
                            "Failed to fetch cross-timeline base {:?} for {:?}",
                            base, current_key
                        )
                    })?;
                    chain.push((
                        current_key,
                        ChainEntry::DeltaWithResolvedBase(data, base_graph),
                    ));
                    break;
                }

                chain.push((current_key.clone(), ChainEntry::Delta(data)));
                current_key = base;
            }
            FrameType::Empty | FrameType::Error => {
                anyhow::bail!(
                    "cannot fetch graph {:?}: frame is {:?}, not Full or Delta",
                    current_key,
                    row.frame_type,
                );
            }
        }
    }

    // Reconstruct forward: the last entry in `chain` is the base (Full or
    // DeltaWithResolvedBase), and we apply deltas in reverse order.
    chain.reverse();

    let mut iter = chain.into_iter();
    let (_, first_entry) = iter.next().expect("chain must have at least one entry");

    let mut current = match first_entry {
        ChainEntry::Full(data) => storage.reconstruct_full_graph(&data).await?,
        ChainEntry::DeltaWithResolvedBase(data, base_graph) => {
            let delta = storage.reconstruct_delta(&data).await?;
            unigraph_core::apply_delta(base_graph, &delta)
                .context("Failed to apply delta on cross-timeline base")?
        }
        ChainEntry::Delta(_) => {
            unreachable!("first entry in reversed chain must be Full or DeltaWithResolvedBase")
        }
    };

    for (entry_key, entry) in iter {
        match entry {
            ChainEntry::Delta(data) => {
                let delta = storage.reconstruct_delta(&data).await?;
                current = unigraph_core::apply_delta(current, &delta)
                    .with_context(|| format!("Failed to apply delta at {:?}", entry_key))?;
            }
            _ => unreachable!("only the first entry can be Full or DeltaWithResolvedBase"),
        }
    }

    Ok(current)
}

/// A link in the delta chain during fetch reconstruction.
enum ChainEntry {
    /// Full frame data — the base of the chain.
    Full(unigraph_storage_core::FrameData),
    /// Delta frame data — needs the preceding entry as base.
    Delta(unigraph_storage_core::FrameData),
    /// Delta whose base is in another timeline — base already resolved.
    DeltaWithResolvedBase(unigraph_storage_core::FrameData, ArrayGraphSerializable),
}

// ---------------------------------------------------------------------------
// Compaction
// ---------------------------------------------------------------------------

/// Compaction is not supported for FullOrDelta timelines.
///
/// Returns `Ok(0)` (no frames converted).
pub async fn compact_timeline(
    _storage: &UnigraphStorage,
    _timeline_id: &TimelineID,
    _start: Option<Timestamp>,
    _end: Option<Timestamp>,
) -> Result<usize> {
    Ok(0)
}

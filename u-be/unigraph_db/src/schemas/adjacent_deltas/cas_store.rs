// Copyright (c) Meta Platforms, Inc. and affiliates.

//! CAS (Compare-And-Swap) store for [`GraphRange`].
//!
//! Stores a [`GraphRange`] atomically with CAS semantics: verifies all
//! target frames are Empty in a transaction, then replaces them with the
//! range's Full and Delta entries. Fails if any target frame is not Empty.
//!
//! ```text
//!   Before:  [Empty]  [Empty]  [Empty]  [Empty]  [Empty]
//!                0        1        2        3        4
//!
//!   After:   [Full]   [Delta]  [Delta]  [Full]   [Delta]
//!               0     base=0   base=1      3     base=3
//! ```
//!
//! # Transaction boundary
//!
//! Expensive work (packing, metric history preparation, blob uploads) happens
//! **before** the transaction. The transaction itself is short: CAS check,
//! delete Empties, insert new frames, store metric history, commit.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::BlobID;
use unigraph_core::apply_delta;
use unigraph_core::pack_delta;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;

use crate::context::UnigraphDbContext;
use crate::frame_storage::PreparedBlobs;
use crate::graph_range::GraphRange;
use crate::graph_range::GraphRangeFrame;

/// A packed entry ready for storage.
struct PackedEntry {
    key: GraphTimeKey,
    frame_type: FrameType,
    base: Option<GraphKey>,
    manifest_json: String,
    blobs: BTreeMap<BlobID, Vec<u8>>,
}

/// Store a [`GraphRange`] atomically with CAS semantics.
///
/// Consumes the range. Packs each entry (Full → `graph.pack()`, Delta →
/// `pack_delta()`), prepares metric history (if enabled), uploads blobs,
/// then stores everything in a single transaction.
pub async fn store_range(
    ctx: &UnigraphDbContext,
    range: GraphRange,
    task: &ll::Task,
) -> Result<()> {
    if range.is_empty() {
        return Ok(());
    }

    let storage = &ctx.storage;
    let (timeline_id, entries) = range.into_entries();

    // -- Check if metric history is enabled --

    let history_enabled = {
        let mut conn = storage.graph.conn().await?;
        let config = conn.get_timeline_config(&timeline_id, task).await?;
        config
            .as_ref()
            .and_then(|c| c.store_metric_history)
            .unwrap_or(false)
    };

    // -- Pack entries + prepare metric history (before transaction) --
    // CPU-heavy: packing involves serialization + compression → off tokio thread.

    let timeline_id_clone = timeline_id.clone();
    let pack_configs: Vec<_> = entries
        .iter()
        .map(|(key, _)| ctx.pack_config_for_key(key))
        .collect();

    let (packed_entries, merged_history) = tokio::task::spawn_blocking(move || {
        let mut packed_entries: Vec<PackedEntry> = Vec::with_capacity(entries.len());
        let mut merged_history: Option<crate::metric_history::PreparedHistoryEntries> = None;
        let mut current_graph: Option<ArrayGraphSerializable> = None;
        let mut prev_graph_id: Option<GraphID> = None;

        for ((key, frame), config) in entries.into_iter().zip(pack_configs) {
            match frame {
                GraphRangeFrame::Full(graph) => {
                    let pack_task = ll::Task::create_new("");
                    let package = graph
                        .pack(&config, &pack_task)
                        .context("Failed to pack graph")?;
                    let manifest_json = serde_json::to_string(&package.manifest)
                        .context("Failed to serialize manifest")?;

                    packed_entries.push(PackedEntry {
                        key: key.clone(),
                        frame_type: FrameType::Full,
                        base: None,
                        manifest_json,
                        blobs: package.blobs,
                    });

                    if history_enabled {
                        merge_history(
                            &mut merged_history,
                            crate::metric_history::prepare_history_entries(&[(
                                key.clone(),
                                &graph,
                            )]),
                        );
                    }

                    prev_graph_id = Some(key.graph_id);
                    current_graph = Some(graph);
                }
                GraphRangeFrame::Delta(delta) => {
                    let package = pack_delta(&delta, &config).context("Failed to pack delta")?;
                    let manifest_json = serde_json::to_string(&package.manifest)
                        .context("Failed to serialize delta manifest")?;

                    let base_graph_id = prev_graph_id.with_context(|| {
                        format!(
                            "delta frame graph_id={} has no preceding frame",
                            key.graph_id.0
                        )
                    })?;

                    packed_entries.push(PackedEntry {
                        key: key.clone(),
                        frame_type: FrameType::Delta,
                        base: Some(GraphKey {
                            timeline_id: timeline_id_clone.clone(),
                            graph_id: base_graph_id,
                        }),
                        manifest_json,
                        blobs: package.blobs,
                    });

                    if history_enabled {
                        let base = current_graph.take().with_context(|| {
                            format!(
                                "delta frame graph_id={} has no preceding full for history",
                                key.graph_id.0,
                            )
                        })?;
                        let graph = apply_delta(base, &delta).with_context(|| {
                            format!(
                                "failed to apply delta at graph_id={} for history",
                                key.graph_id.0,
                            )
                        })?;
                        merge_history(
                            &mut merged_history,
                            crate::metric_history::prepare_history_entries(&[(
                                key.clone(),
                                &graph,
                            )]),
                        );
                        current_graph = Some(graph);
                    }

                    prev_graph_id = Some(key.graph_id);
                }
            }
        }

        Ok::<_, anyhow::Error>((packed_entries, merged_history))
    })
    .await
    .context("spawn_blocking panicked")??;

    // Dedup node names and ensure partitions (before transaction).
    let prepared_history = if let Some(mut prepared) = merged_history {
        for week_entry in prepared.by_week.values_mut() {
            week_entry.all_node_names.sort();
            week_entry.all_node_names.dedup();
        }
        let mut conn = storage.graph.conn().await?;
        crate::metric_history::ensure_history_partitions(&mut *conn, &timeline_id, &prepared, task)
            .await?;
        Some(prepared)
    } else {
        None
    };

    // -- Prepare blobs for all entries in parallel (before transaction) --

    let blob_futs: Vec<_> = packed_entries
        .iter()
        .map(|entry| storage.prepare_blobs_for_storage(&timeline_id, &entry.blobs, task))
        .collect();
    let prepared_blobs: Vec<PreparedBlobs> = futures::future::try_join_all(blob_futs).await?;

    // -- Transaction: CAS check + store --

    let mut conn = storage.graph.conn_write().await?;
    conn.start_transaction(task).await?;
    conn.get_timeline_config_and_lock(&timeline_id, task)
        .await?;

    // CAS check: verify all target frames exist and are Empty.
    let graph_ids: Vec<_> = packed_entries.iter().map(|e| e.key.graph_id).collect();
    let existing = conn
        .select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                graph_ids: Some(graph_ids.clone()),
                with_data: Some(false),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
        .await?;

    if existing.len() != packed_entries.len() {
        anyhow::bail!(
            "CAS check failed: expected {} frames, found {} in timeline '{}'",
            packed_entries.len(),
            existing.len(),
            timeline_id.0,
        );
    }

    for row in &existing {
        if row.frame_type != FrameType::Empty {
            anyhow::bail!(
                "CAS check failed: frame graph_id={} is {:?}, expected Empty \
                 in timeline '{}'",
                row.frame.graph_id.0,
                row.frame_type,
                timeline_id.0,
            );
        }
    }

    // Validate adjacent-delta invariant: each Delta's base must be the
    // immediately preceding frame in the timeline. Fetch all frames in
    // the range's span (single query, metadata only) and verify.
    validate_adjacency(&mut *conn, &timeline_id, &packed_entries, task).await?;

    // Delete all Empty frames.
    for graph_id in &graph_ids {
        let key = GraphKey {
            timeline_id: timeline_id.clone(),
            graph_id: *graph_id,
        };
        conn.delete_frame(&key, task).await?;
    }

    // Store each entry.
    for (entry, prepared) in packed_entries.iter().zip(prepared_blobs.iter()) {
        storage
            .store_package_on_conn(
                &mut *conn,
                &entry.key,
                entry.frame_type.clone(),
                entry.base.as_ref(),
                &entry.manifest_json,
                prepared.inline.as_deref(),
                prepared.external_keys.as_deref(),
                None,
                task,
            )
            .await?;
    }

    // Store metric history (if prepared).
    if let Some(prepared_history) = prepared_history {
        crate::metric_history::store_metric_history_on_conn(
            &mut *conn,
            &timeline_id,
            prepared_history,
            task,
        )
        .await?;
    }

    conn.commit_transaction(task).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate that every Delta in the range has its `base` pointing to the
/// immediately preceding frame in the timeline — not a frame further back.
///
/// This catches callers that chain non-adjacent Empty frames into a single
/// delta range (e.g., when intervening frames were already stored by a
/// previous run).
async fn validate_adjacency(
    conn: &mut dyn unigraph_storage_core::UnigraphGraphConnection,
    timeline_id: &unigraph_storage_core::TimelineID,
    entries: &[PackedEntry],
    task: &ll::Task,
) -> Result<()> {
    let deltas_with_base: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            if e.frame_type == FrameType::Delta {
                e.base.as_ref().map(|b| (&e.key, b.graph_id))
            } else {
                None
            }
        })
        .collect();

    if deltas_with_base.is_empty() {
        return Ok(());
    }

    let first_id = entries.first().unwrap().key.graph_id;
    let last_id = entries.last().unwrap().key.graph_id;

    let span = conn
        .select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                graph_id_bounds: Some((Some(first_id), Some(last_id))),
                with_data: Some(false),
                order: Some(unigraph_storage_core::Order::Asc),
                limit: None,
                frame_types: None,
                timestamp_bounds: None,
                graph_ids: None,
                before: None,
                expires_before: None,
            },
            task,
        )
        .await
        .context("Failed to fetch span for adjacency validation")?;

    let mut preceding: std::collections::HashMap<GraphID, GraphID> =
        std::collections::HashMap::new();
    for pair in span.windows(2) {
        preceding.insert(pair[1].frame.graph_id, pair[0].frame.graph_id);
    }

    for (key, base_graph_id) in &deltas_with_base {
        if let Some(&actual_prev) = preceding.get(&key.graph_id) {
            if actual_prev != *base_graph_id {
                anyhow::bail!(
                    "Adjacent delta invariant violated in timeline '{}': \
                     Delta graph_id={} has base={} but the preceding frame \
                     in the timeline is graph_id={}",
                    timeline_id.0,
                    key.graph_id.0,
                    base_graph_id.0,
                    actual_prev.0,
                );
            }
        }
    }

    Ok(())
}

/// Merge newly prepared history entries into the accumulated result.
fn merge_history(
    merged: &mut Option<crate::metric_history::PreparedHistoryEntries>,
    prepared: crate::metric_history::PreparedHistoryEntries,
) {
    match merged {
        None => *merged = Some(prepared),
        Some(existing) => {
            for (week_key, week_entry) in prepared.by_week {
                existing
                    .by_week
                    .entry(week_key)
                    .and_modify(|existing_week| {
                        existing_week.new_frames.extend(&week_entry.new_frames);
                        for (node_name, entries) in &week_entry.entries_by_node {
                            existing_week
                                .entries_by_node
                                .entry(node_name.clone())
                                .or_default()
                                .extend(entries.iter().map(|(k, v)| (*k, v.clone())));
                        }
                        existing_week
                            .all_node_names
                            .extend(week_entry.all_node_names.iter().cloned());
                    })
                    .or_insert(week_entry);
            }
        }
    }
}

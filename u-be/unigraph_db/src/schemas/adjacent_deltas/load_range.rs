// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Load a [`GraphRange`] from storage.
//!
//! Fetches frames, unpacks each one into domain data (Full →
//! `ArrayGraphSerializable`, Delta → `GraphDelta`), and returns a
//! `GraphRange` holding unpacked graphs and deltas.

use anyhow::Context;
use anyhow::Result;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;

use crate::context::UnigraphDbContext;
use crate::graph_range::GraphRange;
use crate::graph_range::GraphRangeFrame;

/// Load a range of frames from storage into a [`GraphRange`].
///
/// Only Full and Delta frames are loaded (Empty and Error are skipped).
/// The range must start with a Full frame. Each frame is unpacked into
/// domain data: Full → `ArrayGraphSerializable`, Delta → `GraphDelta`.
#[ll::task]
pub async fn load_range(
    timeline_id: &TimelineID,
    ctx: &UnigraphDbContext,
    from: Option<GraphID>,
    to: Option<GraphID>,
    task: &ll::Task,
) -> Result<GraphRange> {
    let storage = &ctx.storage;
    let mut conn = storage.graph.conn().await?;
    let rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                frame_types: Some(vec![FrameType::Full, FrameType::Delta]),
                graph_id_bounds: Some((from, to)),
                order: Some(Order::Asc),
                with_data: Some(true),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    drop(conn);

    // Validate chain before unpacking.
    validate_loaded_rows(&rows, timeline_id)?;
    report_loaded_stats(&rows, &task);

    // Unpack each frame into domain data in parallel.
    let unpack_futs: Vec<_> =
        rows.iter()
            .map(|row| async {
                let data = row.data.as_ref().with_context(|| {
                    format!("frame graph_id={} has no data", row.frame.graph_id.0)
                })?;

                let key = GraphTimeKey {
                    timeline_id: timeline_id.clone(),
                    timestamp: row.frame.timestamp,
                    graph_id: row.frame.graph_id,
                };

                let frame = match &row.frame_type {
                    FrameType::Full => {
                        let graph = storage.reconstruct_full_graph(data, &task).await?;
                        GraphRangeFrame::Full(graph)
                    }
                    FrameType::Delta => {
                        let delta = storage.reconstruct_delta(data, &task).await?;
                        GraphRangeFrame::Delta(Box::new(delta))
                    }
                    other => anyhow::bail!(
                        "unexpected {:?} frame at graph_id={}",
                        other,
                        row.frame.graph_id.0,
                    ),
                };

                Ok::<_, anyhow::Error>((key, frame))
            })
            .collect();
    let entries = futures::future::try_join_all(unpack_futs).await?;

    Ok(GraphRange::from_entries(timeline_id.clone(), entries))
}

/// Record the shape of the chain that was loaded.
///
/// The cost of a load tracks the number of Fulls far more than the number of
/// frames — each one is a separate blob fetch and decompress — so both are
/// worth having next to the duration.
fn report_loaded_stats(rows: &[unigraph_storage_core::FrameRow], task: &ll::Task) {
    let Some((first, last)) = rows.first().zip(rows.last()) else {
        return;
    };
    let fulls = rows
        .iter()
        .filter(|row| row.frame_type == FrameType::Full)
        .count();

    task.data("frames", rows.len());
    task.data("from_graph_id", first.frame.graph_id.0);
    task.data("to_graph_id", last.frame.graph_id.0);
    task.data("fulls", fulls);
    task.data("deltas", rows.len() - fulls);
}

/// Validate that loaded rows form a valid chain.
fn validate_loaded_rows(
    rows: &[unigraph_storage_core::FrameRow],
    timeline_id: &TimelineID,
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    anyhow::ensure!(
        rows[0].frame_type == FrameType::Full,
        "first frame in range must be Full, got {:?} (graph_id={}) in timeline '{}'",
        rows[0].frame_type,
        rows[0].frame.graph_id.0,
        timeline_id.0,
    );

    for i in 1..rows.len() {
        let prev = &rows[i - 1];
        let curr = &rows[i];

        match &curr.frame_type {
            FrameType::Delta => {
                let base = curr.base.as_ref().with_context(|| {
                    format!(
                        "delta frame graph_id={} has no base key",
                        curr.frame.graph_id.0,
                    )
                })?;
                anyhow::ensure!(
                    base.graph_id == prev.frame.graph_id,
                    "delta frame graph_id={} has base graph_id={}, \
                     expected graph_id={} (the preceding frame)",
                    curr.frame.graph_id.0,
                    base.graph_id.0,
                    prev.frame.graph_id.0,
                );
            }
            FrameType::Full => {
                // A Full frame in the middle is fine — starts a new sub-chain.
            }
            other => {
                anyhow::bail!(
                    "unexpected {:?} frame in range at graph_id={}",
                    other,
                    curr.frame.graph_id.0,
                );
            }
        }
    }

    Ok(())
}

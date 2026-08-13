// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Getting graphs out of storage, and metric names into ids.
//!
//! Nothing here makes a decision — it is the machinery that turns a frame into
//! `node -> metric id -> value`, plus the two optimisations that make ingesting
//! a long timeline affordable: chunked replay, and a metric dictionary held for
//! the length of a run.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use unigraph_metric_history::extract_node_metrics;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::TimestampBounds;

use super::GraphHistory;
use super::NodeMetrics;
use super::NodeValues;
use super::ingest::RunState;

impl GraphHistory {
    /// Every frame of the timeline, metadata only, ascending.
    pub(super) async fn select_history_frames(
        &self,
        timeline_id: &TimelineID,
        bounds: TimestampBounds,
        graph_id_bounds: GraphIDBounds,
        task: &ll::Task,
    ) -> Result<Vec<FrameRow>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        conn.select_frames(
            &FrameQuery {
                timeline_id: timeline_id.clone(),
                limit: None,
                frame_types: Some(vec![
                    FrameType::Empty,
                    FrameType::Full,
                    FrameType::Delta,
                    FrameType::Error,
                ]),
                order: Some(Order::Asc),
                timestamp_bounds: Some(bounds),
                graph_id_bounds: Some(graph_id_bounds),
                graph_ids: None,
                with_data: Some(false),
                before: None,
                expires_before: None,
            },
            task,
        )
        .await
    }

    /// Whether this timeline's frames can be replayed as a range.
    ///
    /// Only `AdjacentDeltas` chains a delta to the frame before it, which is
    /// what lets one pass reconstruct every graph. Other schemas fall back to
    /// fetching each frame independently.
    pub(super) async fn timeline_supports_replay(
        &self,
        timeline_id: &TimelineID,
        task: &ll::Task,
    ) -> Result<bool> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let config = conn.get_timeline_config(timeline_id, task).await?;
        Ok(matches!(
            config.map(|config| config.schema),
            Some(TimelineSchema::AdjacentDeltas(_))
        ))
    }

    /// Reconstruct a chunk's graphs in one pass and return their metrics.
    ///
    /// Fetching frames one at a time re-walks from the nearest Full for every
    /// frame, so a chain of length L costs O(L²) delta applications and L round
    /// trips. `load_range` pulls the chain in a single query — snapping back to
    /// the Full itself, so a chunk boundary landing mid-chain costs only the
    /// leading frames the caller then filters out — and `replay` folds each
    /// graph out of the previous one, making it O(L).
    ///
    /// Best-effort: any failure falls back to the per-frame fetch path rather
    /// than failing the run, so an odd chain degrades in speed, not
    /// correctness. The second return value is whether it did.
    pub(super) async fn extract_chunk(
        &self,
        timeline_id: &TimelineID,
        chunk: &[FrameRow],
        replayable: bool,
        task: &ll::Task,
    ) -> (BTreeMap<GraphID, NodeMetrics>, bool) {
        if !replayable {
            return (BTreeMap::new(), false);
        }
        let wanted = chunk
            .iter()
            .map(|frame| frame.frame.graph_id)
            .collect::<BTreeSet<_>>();
        let (Some(from), Some(to)) = (wanted.first(), wanted.last()) else {
            return (BTreeMap::new(), false);
        };

        match self
            .replay_metrics(timeline_id, *from, *to, &wanted, task)
            .await
        {
            Ok(graphs) => (graphs, false),
            Err(error) => {
                task.data("history_replay_fallback", format!("{error:#}"));
                (BTreeMap::new(), true)
            }
        }
    }

    /// Reconstruct the graphs `wanted` names, keyed by graph ID.
    ///
    /// `load_range` widens `from` back to its chain's Full, so the range can
    /// start before `from` — `wanted` is what decides which graphs are kept.
    async fn replay_metrics(
        &self,
        timeline_id: &TimelineID,
        from: GraphID,
        to: GraphID,
        wanted: &BTreeSet<GraphID>,
        task: &ll::Task,
    ) -> Result<BTreeMap<GraphID, NodeMetrics>> {
        let range = crate::schemas::adjacent_deltas::load_range(
            timeline_id,
            &self.ctx,
            Some(from),
            Some(to),
            task,
        )
        .await?;

        let mut graphs = BTreeMap::new();
        range.replay(|key, graph| {
            if wanted.contains(&key.graph_id) {
                graphs.insert(key.graph_id, extract_node_metrics(graph));
            }
            Ok(())
        })?;
        Ok(graphs)
    }

    /// Fall back to reconstructing one frame on its own.
    ///
    /// Used when the chunk replay could not supply the graph — a
    /// non-`AdjacentDeltas` timeline, a range that failed to load, or a
    /// predecessor this run never walked past.
    pub(super) async fn fetch_and_extract(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        task: &ll::Task,
    ) -> Result<NodeMetrics> {
        let graph = self
            .ctx
            .storage
            .fetch_graph(
                &GraphKey {
                    timeline_id: timeline_id.clone(),
                    graph_id,
                },
                task,
            )
            .await?;
        Ok(extract_node_metrics(&graph))
    }

    /// Make sure every metric name in `extracted` has an id, interning only if
    /// one is genuinely new.
    ///
    /// Interning takes the timeline's exclusive lock, so doing it per frame
    /// meant a write transaction per frame for a dictionary that stops changing
    /// after the first one. A stale cache is self-correcting: the only way it
    /// hurts is a missing name, and that is exactly what triggers the re-read.
    pub(super) async fn refresh_metric_ids(
        &self,
        timeline_id: &TimelineID,
        extracted: &NodeMetrics,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<()> {
        let complete = extracted
            .values()
            .flat_map(BTreeMap::keys)
            .all(|name| run.metric_ids.contains_key(name));
        if complete {
            return Ok(());
        }
        run.metric_ids = self
            .intern_metric_names(timeline_id, extracted, task)
            .await?;
        Ok(())
    }

    /// Assign a stable id to every metric name present in `extracted`, and
    /// return the timeline's full `name -> metric_id` dictionary.
    ///
    /// Runs in its own short transaction. New ids are allocated as
    /// `MAX(metric_id) + 1` *at statement execution time*, so two writers
    /// interning different names for the same timeline concurrently would
    /// otherwise be free to compute the same next id. The
    /// `(timeline_id, metric_id)` primary key means that surfaces as a
    /// constraint error rather than silent corruption, but serialising the
    /// allocation avoids the intermittent failure entirely — and makes the
    /// single-writer requirement explicit instead of leaning on whatever
    /// exclusivity `conn_write()` happens to provide on a given backend.
    ///
    /// Deliberately kept out of the per-frame insert transaction: interning
    /// only ever appends to a tiny dictionary, so committing it early is safe
    /// even if the frame itself later fails.
    async fn intern_metric_names(
        &self,
        timeline_id: &TimelineID,
        extracted: &NodeMetrics,
        task: &ll::Task,
    ) -> Result<BTreeMap<String, u32>> {
        let names = extracted
            .values()
            .flat_map(|metrics| metrics.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        let metric_ids = conn
            .intern_history_metrics(timeline_id, &names, task)
            .await?;
        conn.commit_transaction(task).await?;
        Ok(metric_ids)
    }

    /// Swap metric names for their interned ids.
    pub(super) fn to_node_values(
        &self,
        extracted: &NodeMetrics,
        run: &RunState,
    ) -> Result<NodeValues> {
        extracted
            .iter()
            .map(|(node_name, metrics)| {
                let values = metrics
                    .iter()
                    .map(|(metric_name, value)| {
                        let metric_id = run.metric_ids.get(metric_name).ok_or_else(|| {
                            anyhow::anyhow!("metric was not interned: {metric_name}")
                        })?;
                        Ok((*metric_id, *value))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                Ok((node_name.clone(), values))
            })
            .collect()
    }
}

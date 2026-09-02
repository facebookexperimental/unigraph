// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Judging one frame against the frame before it.
//!
//! # The whole rule
//!
//! ```text
//! prev = the immediately preceding BUILT frame
//!
//! no prev at all        -> every node gets FIRST
//! a gap sits behind us  -> no reasons; the frame's AFTER_GAP flag holds every row
//! otherwise, per node   -> |v - v_prev| >= threshold  =>  OVER_THRESHOLD here,
//!                                                         ANCHOR at prev
//! and at the newest built frame, every node also gets LATEST
//! ```
//!
//! Everything else in this file is bookkeeping around those five lines.
//!
//! # Why the anchor is written from memory
//!
//! A crossing's step is only readable if the frame before it has a row, and the
//! threshold has usually folded that row away. Nothing in storage remembers
//! what a folded-away frame held, so the run carries the previous frame's
//! values ([`PreviousFrame`]) and writes the row after the fact. A run starting
//! mid-timeline re-derives them with one graph fetch — which is the normal
//! shape for a scheduled job ingesting a frame or two per pass, and without it
//! no anchor would ever be written at all.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;

use anyhow::Result;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::HistoryEntryRow;
use unigraph_storage_core::HistoryStatusRow;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;

use super::GraphHistory;
use super::HistoryIngestOptions;
use super::HistoryIngestReport;
use super::NodeMetrics;
use super::NodeValues;
use super::context::FrameContext;
use crate::graph_history::ErrorPayload;
use crate::graph_history::FrameFlags;
use crate::graph_history::IngestState;
use crate::graph_history::Reasons;
use crate::graph_history::Values;
use crate::graph_history::crosses;
use crate::graph_history::encode_values;
use crate::graph_history::only_frame;

/// How many frames one `load_range` + replay pass covers.
///
/// Replaying a range reconstructs each graph from the previous one, so the
/// whole chunk's unpacked frames are resident while it runs. Bounding the chunk
/// bounds that memory; the only cost of a smaller chunk is re-walking from the
/// preceding Full once per chunk.
const REPLAY_CHUNK_FRAMES: usize = 200;

/// Mutable state carried across the frames of a single ingest run.
pub(super) struct RunState {
    /// The last data frame this run judged. Usable only when it really is the
    /// immediate predecessor of the frame in hand — checked rather than
    /// assumed, because a work list has holes in it.
    pub(super) previous: Option<PreviousFrame>,
    /// The timeline's `metric name -> id` dictionary.
    ///
    /// Held across the run because interning takes the timeline's exclusive
    /// lock, and after the first frame it has nothing to do — the dictionary is
    /// a handful of names that never change.
    pub(super) metric_ids: BTreeMap<String, u32>,
    /// The run's metric allowlist. See [`HistoryIngestOptions::metrics`].
    pub(super) metrics: Option<BTreeSet<String>>,
}

impl RunState {
    /// Does this run record `metric_name`? Everything, unless it named some.
    pub(super) fn records(&self, metric_name: &str) -> bool {
        self.metrics
            .as_ref()
            .is_none_or(|names| names.contains(metric_name))
    }

    /// The recorded metric names present anywhere in `extracted`.
    pub(super) fn recorded_names(&self, extracted: &NodeMetrics) -> Vec<String> {
        extracted
            .values()
            .flat_map(BTreeMap::keys)
            .filter(|name| self.records(name))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// The previous built frame, as far as this run knows it.
///
/// Costs one packed value map per node. That is the price of being able to
/// write the predecessor's row after the fact.
pub(super) struct PreviousFrame {
    pub(super) graph_id: GraphID,
    pub(super) timestamp: Timestamp,
    pub(super) values: NodeValues,
    /// Nodes that already have a row here, and so need a reason bit added
    /// rather than a whole new row.
    pub(super) rows_present: HashSet<String>,
}

/// What one frame's verdict comes to.
struct FramePlan {
    /// The rows to write at this frame.
    rows: Vec<HistoryEntryRow>,
    /// Nodes whose crossing here needs the previous frame's row kept.
    anchors: Vec<String>,
    /// This frame bounds a gap, so its zero-reason rows are load-bearing.
    barrier: bool,
}

/// What one frame's commit actually wrote.
struct FrameCounts {
    entries: usize,
    anchors: usize,
    barrier_rows: usize,
}

impl GraphHistory {
    /// Walk the work list in `graph_id` order, judging what needs it.
    pub(super) async fn ingest_frames(
        &self,
        timeline_id: &TimelineID,
        work: &[FrameRow],
        context: &FrameContext,
        options: &HistoryIngestOptions,
        report: &mut HistoryIngestReport,
        task: &ll::Task,
    ) -> Result<()> {
        let total = i64::try_from(work.len())?;
        task.data("frames_to_ingest", total);
        task.progress(0, total);

        let replayable = self.timeline_supports_replay(timeline_id, task).await?;
        let mut run = RunState {
            previous: None,
            metric_ids: BTreeMap::new(),
            metrics: options.metrics.clone(),
        };
        let mut done = 0i64;

        for chunk in work.chunks(REPLAY_CHUNK_FRAMES) {
            let (mut graphs, fell_back) = self
                .extract_chunk(timeline_id, chunk, replayable, task)
                .await;
            report.replay_fallbacks += usize::from(fell_back);

            for frame in chunk {
                let extracted = graphs.remove(&frame.frame.graph_id);
                self.ingest_one_frame(
                    timeline_id,
                    frame,
                    context,
                    options.threshold,
                    extracted,
                    &mut run,
                    report,
                    task,
                )
                .await;
                done += 1;
                task.progress(done, total);
            }
        }
        Ok(())
    }

    /// Judge one frame, recording either its verdict or its failure.
    #[expect(
        clippy::too_many_arguments,
        reason = "a params struct here would just re-list the same borrows with a name"
    )]
    async fn ingest_one_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        context: &FrameContext,
        threshold: f64,
        extracted: Option<NodeMetrics>,
        run: &mut RunState,
        report: &mut HistoryIngestReport,
        task: &ll::Task,
    ) {
        match self
            .judge_frame(timeline_id, frame, context, threshold, extracted, run, task)
            .await
        {
            Ok(counts) => {
                report.ingested += 1;
                report.entries += counts.entries;
                report.anchors += counts.anchors;
                report.barrier_rows += counts.barrier_rows;
            }
            Err(error) => {
                report.errors += 1;
                // The run no longer knows what this frame held, so the next one
                // must re-derive its predecessor rather than trust a stale one.
                run.previous = None;
                if let Err(record_error) = self
                    .record_error(timeline_id, frame, context, error, task)
                    .await
                {
                    task.data(
                        "history_error_status_failure",
                        format!("{}: {record_error:#}", frame.frame.graph_id.0),
                    );
                }
            }
        }
    }

    /// Fetch, extract, judge and commit one frame.
    #[expect(
        clippy::too_many_arguments,
        reason = "a params struct here would just re-list the same borrows with a name"
    )]
    async fn judge_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        context: &FrameContext,
        threshold: f64,
        extracted: Option<NodeMetrics>,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<FrameCounts> {
        let graph_id = frame.frame.graph_id;
        let extracted = match extracted {
            Some(extracted) => extracted,
            None => self.fetch_and_extract(timeline_id, graph_id, task).await?,
        };
        self.refresh_metric_ids(timeline_id, &extracted, run, task)
            .await?;
        let current = self.to_node_values(&extracted, run)?;

        let previous = self
            .previous_frame(timeline_id, frame, context, run, task)
            .await?;
        let plan = plan_frame(
            frame,
            &current,
            previous.as_ref(),
            context.flags(graph_id),
            context.newest_data_frame() == Some(graph_id),
            threshold,
        );
        let counts = self
            .commit_frame(timeline_id, frame, context, &plan, previous.as_ref(), task)
            .await?;

        run.previous = Some(PreviousFrame {
            graph_id,
            timestamp: frame.frame.timestamp,
            rows_present: plan.rows.iter().map(|row| row.node_name.clone()).collect(),
            values: current,
        });
        Ok(counts)
    }

    /// The values this frame's verdicts are measured against.
    ///
    /// `None` in the two cases where there is nothing to measure against: this
    /// is the first frame with data, or a gap sits behind it — in which case
    /// the frame is a barrier and keeps every row regardless, so reaching
    /// across the hole would buy nothing.
    async fn previous_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        context: &FrameContext,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<Option<PreviousFrame>> {
        let graph_id = frame.frame.graph_id;
        if context.flags(graph_id).contains(FrameFlags::AFTER_GAP) {
            return Ok(None);
        }
        let Some(preceding) = context.preceding_data_frame(graph_id) else {
            return Ok(None);
        };

        match run.previous.take() {
            Some(previous) if previous.graph_id == preceding => Ok(Some(previous)),
            _ => {
                self.load_previous_frame(timeline_id, preceding, context, run, task)
                    .await
            }
        }
    }

    /// Reconstruct a predecessor this run never saw. One graph fetch.
    async fn load_previous_frame(
        &self,
        timeline_id: &TimelineID,
        graph_id: GraphID,
        context: &FrameContext,
        run: &mut RunState,
        task: &ll::Task,
    ) -> Result<Option<PreviousFrame>> {
        task.data("history_previous_frame_refetch", graph_id.0);
        let extracted = self.fetch_and_extract(timeline_id, graph_id, task).await?;
        self.refresh_metric_ids(timeline_id, &extracted, run, task)
            .await?;

        let mut conn = self.ctx.storage.graph.conn().await?;
        let rows_present = conn
            .get_history_entries_at(timeline_id, graph_id, task)
            .await?
            .into_iter()
            .map(|sample| sample.node_name)
            .collect();
        drop(conn);

        Ok(Some(PreviousFrame {
            graph_id,
            timestamp: context
                .timestamp_of(graph_id)
                .unwrap_or_else(Timestamp::now),
            values: self.to_node_values(&extracted, run)?,
            rows_present,
        }))
    }

    /// Write one frame's rows, its predecessor's anchors and its checkpoint in
    /// a single transaction.
    ///
    /// Committing the anchors alongside the frame that needed them is what
    /// guarantees a crossing is never left without the row that explains it.
    async fn commit_frame(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        context: &FrameContext,
        plan: &FramePlan,
        previous: Option<&PreviousFrame>,
        task: &ll::Task,
    ) -> Result<FrameCounts> {
        let graph_id = frame.frame.graph_id;
        let (new_anchors, existing_anchors) = split_anchors(previous, &plan.anchors);
        let stale_blob = context
            .error_blob_key(graph_id)
            .into_iter()
            .collect::<Vec<_>>();

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;

        conn.insert_history_entries(timeline_id, &plan.rows, task)
            .await?;
        conn.insert_history_entries(timeline_id, &new_anchors, task)
            .await?;
        if let Some(previous) = previous
            && !existing_anchors.is_empty()
        {
            conn.set_history_reasons_at(
                timeline_id,
                previous.graph_id,
                &existing_anchors,
                Reasons::ANCHOR,
                Reasons::empty(),
                task,
            )
            .await?;
        }
        // Whatever this frame held from an earlier judgement that no longer has
        // a reason. A barrier is exempt: its zero-reason rows are the point.
        if !plan.barrier {
            conn.delete_collapsed_history_entries(timeline_id, &only_frame(graph_id), task)
                .await?;
        }
        conn.upsert_history_status(
            timeline_id,
            &[HistoryStatusRow {
                graph_id,
                ingest_state: IngestState::Ingested,
                attempts: context.attempts(graph_id),
                error_blob_key: None,
                frame_flags: context.flags(graph_id),
            }],
            task,
        )
        .await?;
        conn.register_blobs_for_cleanup(&stale_blob, task).await?;
        conn.commit_transaction(task).await?;

        Ok(FrameCounts {
            entries: plan.rows.len(),
            anchors: new_anchors.len() + existing_anchors.len(),
            barrier_rows: match plan.barrier {
                true => plan.rows.len(),
                false => 0,
            },
        })
    }

    async fn record_error(
        &self,
        timeline_id: &TimelineID,
        frame: &FrameRow,
        context: &FrameContext,
        error: anyhow::Error,
        task: &ll::Task,
    ) -> Result<()> {
        let graph_id = frame.frame.graph_id;
        let previous_key = context.error_blob_key(graph_id);
        let blob_key = format!("{}/{}/history_error", timeline_id.0, graph_id.0);
        self.put_error_blob(&blob_key, previous_key.as_deref(), &error, task)
            .await?;

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.upsert_history_status(
            timeline_id,
            &[HistoryStatusRow {
                graph_id,
                ingest_state: IngestState::Failed,
                attempts: context.attempts(graph_id) + 1,
                error_blob_key: Some(blob_key.clone()),
                frame_flags: context.flags(graph_id),
            }],
            task,
        )
        .await?;
        conn.unregister_blobs_for_cleanup(&[blob_key], task).await?;
        conn.commit_transaction(task).await
    }

    async fn put_error_blob(
        &self,
        blob_key: &str,
        previous_key: Option<&str>,
        error: &anyhow::Error,
        task: &ll::Task,
    ) -> Result<()> {
        let payload = ErrorPayload {
            messages: vec![error.to_string()],
            details: Some(format!("{error:#}")),
        };
        let data = serde_json::to_vec(&payload)
            .map_err(|error| anyhow::anyhow!("failed to serialize ErrorPayload: {error}"))?;
        if previous_key != Some(blob_key) {
            let mut conn = self.ctx.storage.graph.conn_write().await?;
            conn.register_blobs_for_cleanup(&[blob_key.to_string()], task)
                .await?;
        }
        self.ctx.storage.blob.put_blob(blob_key, &data).await
    }

    /// Move the `LATEST` pin off the frame that used to be newest.
    ///
    /// The new holder was given the reason as it was judged. This only has to
    /// release the old one, and only when the run actually reached the newest
    /// frame — a pass bounded by `--max-id` leaves the pin where it is.
    pub(super) async fn refresh_latest(
        &self,
        timeline_id: &TimelineID,
        context: &FrameContext,
        previously_latest: Option<GraphID>,
        task: &ll::Task,
    ) -> Result<()> {
        let (Some(newest), Some(previous)) = (context.newest_data_frame(), previously_latest)
        else {
            return Ok(());
        };
        if newest == previous {
            return Ok(());
        }

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        let released = conn
            .clear_history_reasons_at(timeline_id, previous, Reasons::LATEST, task)
            .await?;
        conn.commit_transaction(task).await?;
        task.data("latest_pin_released", i64::try_from(released)?);
        Ok(())
    }
}

/// Decide what one frame's rows are and why.
///
/// Pure, so the interesting part of ingestion is testable without a database.
fn plan_frame(
    frame: &FrameRow,
    current: &NodeValues,
    previous: Option<&PreviousFrame>,
    flags: FrameFlags,
    is_newest: bool,
    threshold: f64,
) -> FramePlan {
    let barrier = flags.is_barrier();
    // Across a gap there is nothing to measure against and nothing to claim.
    // The frame's AFTER_GAP flag holds every row, and the region behind it is
    // marked unknown rather than dressed up as a node's first sample.
    let across_gap = flags.contains(FrameFlags::AFTER_GAP);
    let pinned = match is_newest {
        true => Reasons::LATEST,
        false => Reasons::empty(),
    };
    let absent = Values::new();

    let mut rows = Vec::new();
    let mut anchors = Vec::new();
    for node_name in evaluated_nodes(current, previous) {
        let values = current.get(node_name);
        let (reasons, anchored) = match across_gap {
            true => (Reasons::empty(), false),
            false => judge_node(node_name, values, previous, threshold),
        };
        let reasons = reasons | pinned;

        if !reasons.is_empty() || barrier {
            rows.push(HistoryEntryRow {
                node_name: node_name.clone(),
                graph_id: frame.frame.graph_id,
                timestamp: frame.frame.timestamp,
                values: encode_values(values.unwrap_or(&absent)),
                reasons,
            });
        }
        if anchored {
            anchors.push(node_name.clone());
        }
    }

    FramePlan {
        rows,
        anchors,
        barrier,
    }
}

/// Why this node's row would exist here, and whether the previous frame's row
/// has to stay to explain it.
///
/// Only called where there is an adjacent predecessor to measure against — the
/// across-a-gap case never reaches here.
fn judge_node(
    node_name: &str,
    values: Option<&Values>,
    previous: Option<&PreviousFrame>,
    threshold: f64,
) -> (Reasons, bool) {
    // No predecessor at all means this is the timeline's first built frame.
    let Some(previous) = previous else {
        return (Reasons::FIRST, false);
    };

    let before = previous.values.get(node_name);
    if before.is_none() {
        // The node was not in the previous frame. Treating that as its first
        // sample over-keeps when a node vanishes and comes back, which is the
        // safe direction — and arguably the right one, since a node
        // reappearing is exactly as noteworthy as one arriving.
        return (Reasons::FIRST, false);
    }
    match crosses(before, values, threshold) {
        true => (Reasons::OVER_THRESHOLD, true),
        false => (Reasons::empty(), false),
    }
}

/// Every node either frame knows about.
///
/// The union rather than just this frame's nodes, so a node that dropped out of
/// the graph records a zeroed sample — a real event, and one the threshold will
/// happily call a crossing.
fn evaluated_nodes<'a>(
    current: &'a NodeValues,
    previous: Option<&'a PreviousFrame>,
) -> impl Iterator<Item = &'a String> {
    let vanished = previous
        .into_iter()
        .flat_map(|previous| previous.values.keys())
        .filter(|node_name| !current.contains_key(*node_name));
    current.keys().chain(vanished)
}

/// Split the nodes needing an anchor into rows to write and rows to flag.
///
/// A node whose predecessor row already exists gets a reason bit OR-ed in —
/// never an overwrite, because that row may well be a threshold crossing in its
/// own right, which under this design coexists with being an anchor.
fn split_anchors(
    previous: Option<&PreviousFrame>,
    anchors: &[String],
) -> (Vec<HistoryEntryRow>, Vec<String>) {
    let Some(previous) = previous else {
        return (Vec::new(), Vec::new());
    };

    let mut rows = Vec::new();
    let mut flags = Vec::new();
    for node_name in anchors {
        if previous.rows_present.contains(node_name) {
            flags.push(node_name.clone());
            continue;
        }
        // Nodes the previous frame knew nothing about are skipped: this node's
        // sample is its first, which already reads correctly, and a row of
        // zeros for every newly appearing node would double the cost of a
        // growing graph for nothing.
        let Some(values) = previous.values.get(node_name) else {
            continue;
        };
        rows.push(HistoryEntryRow {
            node_name: node_name.clone(),
            graph_id: previous.graph_id,
            timestamp: previous.timestamp,
            values: encode_values(values),
            reasons: Reasons::ANCHOR,
        });
    }
    (rows, flags)
}

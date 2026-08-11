// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_db::HistoryIngestOptions;
use unigraph_db::graph_history::DEFAULT_SETTLE_HOURS;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Record per-node metric history for recently-landed frames.
///
/// Runs after the fact (typically on a schedule) over the frames in the
/// lookback window, keeping only samples where some metric moved by at least
/// `--threshold` since that node's last *kept* sample. Each frame is fetched
/// and extracted outside any transaction, then its rows and checkpoint are
/// committed in one short per-frame transaction, so a run can never grow into
/// a single oversized write.
///
/// Source frames are registered in order but built out of order, so a frame
/// can be ingested while an earlier one is still unbuilt. Omitting a sample in
/// that situation is unsafe — the frame that fills the hole later could have
/// changed the verdict, and an omission is never revisited. So a frame sitting
/// behind an unfilled hole keeps every row and is flagged for `history
/// compact` to re-threshold once the hole closes. `--settle-hours` decides how
/// long to wait before giving up on a hole ever closing.
///
/// Keeping a sample also writes an *anchor*: the row for the built frame
/// immediately before it, which the threshold had folded away. Without it the
/// sample's step reads as all the drift since the node's last kept row —
/// hundreds of diffs' worth — instead of what the one graph that crossed the
/// threshold actually contributed.
///
/// Already-ingested frames are skipped, so re-running with a different
/// `--threshold` does nothing to them — use `history compact` for that.
///
/// ```sh
/// unigraph history ingest --timeline-id my_timeline --lookback-hours 72 --threshold 1000
/// ```
#[derive(Parser, Debug)]
pub struct HistoryIngest {
    /// Timeline to ingest history for
    #[arg(long)]
    timeline_id: String,

    /// How far back from now to look for frames to ingest
    #[arg(long)]
    lookback_hours: usize,

    /// Minimum absolute change in any metric, versus the node's last kept
    /// sample, for a new sample to be recorded
    #[arg(long)]
    threshold: f64,

    /// How long an unfilled frame may block its successors from being
    /// threshold-filtered before it is presumed abandoned. Set this from the
    /// source pipeline's worst-case catch-up latency, not the job cadence.
    #[arg(long, default_value_t = DEFAULT_SETTLE_HOURS)]
    settle_hours: usize,

    /// Only ingest frames with graph ID >= this value (inclusive). For
    /// repairing a specific range after a `history delete`.
    #[arg(long)]
    min_id: Option<i64>,

    /// Only ingest frames with graph ID <= this value (inclusive)
    #[arg(long)]
    max_id: Option<i64>,
}

impl HistoryIngest {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let report = ctx
            .db
            .graph_history
            .ingest(
                &TimelineID(self.timeline_id.clone()),
                &HistoryIngestOptions {
                    lookback_hours: self.lookback_hours,
                    settle_hours: self.settle_hours,
                    threshold: self.threshold,
                    graph_id_bounds: (self.min_id.map(GraphID), self.max_id.map(GraphID)),
                },
                task,
            )
            .await?;
        ctx.println_after_done(&format!(
            "processed={} omitted={} empty={} skipped={} errors={} entries={} \
             deferred={} deferred_rows={} anchors={}",
            report.processed,
            report.omitted,
            report.empty,
            report.skipped,
            report.errors,
            report.entries,
            report.deferred,
            report.deferred_rows,
            report.anchors,
        ))?;
        Ok(())
    }
}

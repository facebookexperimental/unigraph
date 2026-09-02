// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_db::HistoryIngestOptions;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Record per-node metric history for frames that have not been judged yet.
///
/// A node's row is written where the node **moved by at least `--threshold`
/// against the immediately preceding built frame**. That is a statement about
/// one diff: the series answers "which diff moved this bucket?", not "what was
/// the size over time". Slow creep — a little more every frame, forever — is
/// deliberately not recorded, because no single diff is to blame for it; the
/// newest frame is pinned instead, so the right-hand edge of a chart is always
/// the true current value.
///
/// Source frames are registered in order but built out of order, so the
/// timeline is pocked with holes. A hole costs two things and nothing else: the
/// built frames on either side keep a row for every node, bounding the region
/// where nothing can be attributed, and the frame after it is judged again once
/// the hole closes. Nothing else in the timeline is affected, and nothing is
/// ever provisional.
///
/// Every frame that is not yet ingested stays on the work list with no time
/// bound, so an outage is a delay rather than a hole. `--lookback-hours` only
/// caps how much of that backlog one run will chew through.
///
/// `--metric` narrows what is judged and stored. The threshold is an OR across
/// metrics, so one metric that moves for reasons no diff caused is enough to
/// pull every node's row in at nearly every frame — a WWW budget node's 30-day
/// route load count sits next to its tier sizes and does exactly that. Name the
/// metrics the series is about and the rest cost nothing: they are dropped
/// before interning, so no dictionary entry, no stored value, no verdict.
///
/// Already-ingested frames are skipped, so re-running with a different
/// `--threshold` or `--metric` does nothing to them — use `history compact` for
/// a threshold. Compaction cannot narrow metrics, since it re-judges from the
/// values already stored; that takes a `history delete` and a re-ingest.
///
/// ```sh
/// unigraph history ingest --timeline-id my_timeline --threshold 1000
///
/// # Judge on bundle size only, ignoring the prod counters on the same nodes
/// unigraph history ingest --timeline-id www-budget --threshold 1000 \
///   --metric t1 --metric t2 --metric t3 --metric startup
/// ```
#[derive(Parser, Debug)]
pub struct HistoryIngest {
    /// Timeline to ingest history for
    #[arg(long)]
    timeline_id: String,

    /// Minimum absolute change in any metric, against the immediately
    /// preceding built frame, for a row to be recorded
    #[arg(long)]
    threshold: f64,

    /// Cap how far back this run reaches for outstanding frames. Advisory:
    /// frames it skips stay on the work list for the next run. Defaults to no
    /// limit.
    #[arg(long)]
    lookback_hours: Option<usize>,

    /// Only ingest frames with graph ID >= this value (inclusive). For
    /// repairing a specific range after a `history delete`.
    #[arg(long)]
    min_id: Option<i64>,

    /// Only ingest frames with graph ID <= this value (inclusive)
    #[arg(long)]
    max_id: Option<i64>,

    /// Record only this metric. Repeat for more. Defaults to every metric the
    /// graph carries.
    #[arg(long = "metric")]
    metrics: Vec<String>,
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
                    threshold: self.threshold,
                    graph_id_bounds: (self.min_id.map(GraphID), self.max_id.map(GraphID)),
                    // No `--metric` means every metric, not none of them.
                    metrics: match self.metrics.is_empty() {
                        true => None,
                        false => Some(self.metrics.iter().cloned().collect()),
                    },
                },
                task,
            )
            .await?;
        ctx.println_after_done(&format!(
            "ingested={} no_data={} skipped={} errors={} entries={} anchors={} \
             barrier_rows={} flags_updated={} rejudged={}",
            report.ingested,
            report.no_data,
            report.skipped,
            report.errors,
            report.entries,
            report.anchors,
            report.barrier_rows,
            report.flags_updated,
            report.rejudged,
        ))?;
        Ok(())
    }
}

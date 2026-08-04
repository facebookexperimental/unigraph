// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_db::HistoryCompactOptions;
use unigraph_db::graph_history::DEFAULT_SETTLE_HOURS;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::UnigraphCLIContext;

/// Re-apply a threshold to history that has already been ingested.
///
/// Two reasons to run this. Raising the threshold after the fact is one.
/// The other, and the one that matters on a schedule: `history ingest` cannot
/// safely drop a sample recorded while an earlier frame was still unbuilt, so
/// it keeps everything and flags the frame. `--deferred-only` compacts exactly
/// those flagged ranges once the holes have closed, which is what a job
/// running behind `ingest` wants.
///
/// Only the settled prefix of the timeline is touched — dropping a row is
/// irreversible, so frames that might still change are left alone. Expect
/// compaction to trail the timeline head by roughly `--settle-hours`.
/// Idempotent: a second run at the same threshold drops nothing.
///
/// ```sh
/// # Scheduled: reclaim what ingest had to over-keep
/// unigraph history compact --timeline-id my_timeline --threshold 1000 --deferred-only
///
/// # One-off: raise the threshold across the whole timeline
/// unigraph history compact --timeline-id my_timeline --threshold 5000
/// ```
#[derive(Parser, Debug)]
pub struct HistoryCompact {
    /// Timeline to compact history for
    #[arg(long)]
    timeline_id: String,

    /// Minimum absolute change in any metric, versus the previous kept sample,
    /// for a sample to survive
    #[arg(long)]
    threshold: f64,

    /// Compact only the ranges `ingest` flagged as deferred, ignoring
    /// `--min-id` / `--max-id`
    #[arg(long)]
    deferred_only: bool,

    /// How long an unfilled frame may hold back the settled frontier before it
    /// is presumed abandoned. Must match the value `history ingest` uses.
    #[arg(long, default_value_t = DEFAULT_SETTLE_HOURS)]
    settle_hours: usize,

    /// Start of the graph ID range (inclusive). Defaults to the whole timeline.
    #[arg(long)]
    min_id: Option<i64>,

    /// End of the graph ID range (inclusive). Clamped to the settled frontier.
    #[arg(long)]
    max_id: Option<i64>,

    /// Start of the time range (RFC 3339, e.g. 2025-01-01T00:00:00Z). Defaults to beginning of time.
    #[arg(long)]
    start: Option<String>,

    /// End of the time range (RFC 3339, e.g. 2025-12-31T23:59:59Z). Defaults to end of time.
    #[arg(long)]
    end: Option<String>,
}

impl HistoryCompact {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let report = ctx
            .db
            .graph_history
            .compact(
                &TimelineID(self.timeline_id.clone()),
                &HistoryCompactOptions {
                    threshold: self.threshold,
                    settle_hours: self.settle_hours,
                    range: HistoryRange {
                        timestamps: parse_bounds(self.start.as_deref(), self.end.as_deref())?,
                        graph_ids: (self.min_id.map(GraphID), self.max_id.map(GraphID)),
                    },
                    deferred_only: self.deferred_only,
                },
                task,
            )
            .await?;
        // `None` means the range resolved to nothing, which has several
        // causes — no settled frames, no flagged frames, or a request sitting
        // entirely above the frontier. The task log says which.
        ctx.println_after_done(&format!(
            "checked {} node(s), dropped {} history row(s), compacted through {}.",
            report.nodes,
            report.dropped,
            report
                .compacted_through
                .map_or_else(|| "nothing".to_string(), |id| id.0.to_string()),
        ))?;
        Ok(())
    }
}

pub(crate) fn parse_bounds(
    start: Option<&str>,
    end: Option<&str>,
) -> anyhow::Result<TimestampBounds> {
    Ok(TimestampBounds {
        start: start.map(Timestamp::from_rfc3339).transpose()?,
        end: end.map(Timestamp::from_rfc3339).transpose()?,
    })
}

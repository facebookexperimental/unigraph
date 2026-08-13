// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_db::HistoryCompactOptions;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::UnigraphCLIContext;

/// Reclaim history rows nothing needs any more, and re-apply a threshold.
///
/// Two jobs, with very different costs:
///
/// - **The segment sweep** deletes rows that have no reason left to exist. The
///   usual source is a hole closing: the built frames on either side of a gap
///   keep a row for every node while the region between them is unknown, and
///   once it is not, those rows are ordinary collapse candidates. One statement
///   per stretch between barriers, for every node at once — cheap enough to run
///   on the same schedule as `ingest`.
/// - **The re-threshold pass** re-derives each node's crossings and anchors
///   from the stored values. Only a threshold change needs it, and it costs one
///   series read per node, so bound it with `--min-id` / `--max-id` on a wide
///   timeline.
///
/// Compaction can only ever raise the threshold. Every crossing stores the row
/// before it, so "would this still cross at a higher bar?" is answerable from
/// the rows alone — but lowering the bar needs values that were never written.
/// Re-ingest for that.
///
/// Unlike the design this replaced, there is no settled frontier and no waiting
/// period: each stretch between barriers is judgeable on its own, so compaction
/// reaches the head of the timeline whether or not any hole ever closes.
/// Idempotent: a second run at the same threshold changes nothing.
///
/// ```sh
/// # Scheduled, alongside ingest
/// unigraph history compact --timeline-id my_timeline --threshold 1000
///
/// # One-off: raise the threshold across the whole timeline
/// unigraph history compact --timeline-id my_timeline --threshold 5000
/// ```
#[derive(Parser, Debug)]
pub struct HistoryCompact {
    /// Timeline to compact history for
    #[arg(long)]
    timeline_id: String,

    /// Minimum absolute change in any metric, against the immediately
    /// preceding built frame, for a row to survive
    #[arg(long)]
    threshold: f64,

    /// Start of the graph ID range (inclusive). Defaults to the whole timeline.
    #[arg(long)]
    min_id: Option<i64>,

    /// End of the graph ID range (inclusive). Defaults to the whole timeline.
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
                    range: HistoryRange {
                        timestamps: parse_bounds(self.start.as_deref(), self.end.as_deref())?,
                        graph_ids: (self.min_id.map(GraphID), self.max_id.map(GraphID)),
                    },
                },
                task,
            )
            .await?;
        ctx.println_after_done(&format!(
            "swept {} segment(s) reclaiming {} row(s); re-thresholded {} node(s), \
             dropping {} row(s) and rewriting {}. \
             {} frame flag(s) updated, {} frame(s) queued for re-judgement.",
            report.segments,
            report.collapsed,
            report.nodes,
            report.dropped,
            report.updated,
            report.flags_updated,
            report.rejudged,
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

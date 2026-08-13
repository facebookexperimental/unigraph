// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;

use clap::Parser;
use unigraph_db::HistorySeriesRow;
use unigraph_db::graph_history::Reasons;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;
use crate::history::compact::parse_bounds;

/// Print one node's recorded history, with metric names resolved.
///
/// # Reading the output
///
/// A row's metrics are absolute. The gap between two printed rows is therefore
/// **not** the work of the diff on the second row unless the two are
/// frame-adjacent — and after compaction they usually are not.
///
/// `Δ vs prev frame` is the honest attribution: it is printed exactly when the
/// row above is the immediately preceding built frame, so the difference is one
/// diff's contribution and nothing else. Where that is not the case, the run of
/// frames in between is called out on its own line rather than quietly
/// differenced, because the two reasons for it are very different and neither
/// supports blaming one diff:
///
/// - the frames in between moved the node by less than the threshold each time,
///   so nothing was recorded — real information, just not attributable;
/// - or they carry no data at all (unbuilt or failed), in which case the rows
///   on either side are `[gap-edge]` and the region is genuinely unknown.
///
/// # Reason tags
///
/// ```text
/// [CROSSING]   moved by at least the threshold against the frame before it
/// [anchor]     kept so the crossing after it reads as one diff's work
/// [first]      the node's first recorded sample
/// [latest]     the newest built frame — the node's current value
/// [gap-edge]   kept only to bound a region with no data
/// ```
///
/// ```sh
/// unigraph history show --timeline-id my_timeline --node-name my_node
/// ```
#[derive(Parser, Debug)]
pub struct HistoryShow {
    /// Timeline to read history from
    #[arg(long)]
    timeline_id: String,

    /// Node whose series to print
    #[arg(long)]
    node_name: String,

    /// Start of the time range (RFC 3339, e.g. 2025-01-01T00:00:00Z). Defaults to beginning of time.
    #[arg(long)]
    start: Option<String>,

    /// End of the time range (RFC 3339, e.g. 2025-12-31T23:59:59Z). Defaults to end of time.
    #[arg(long)]
    end: Option<String>,
}

/// Width of the rule the unknown-region banner is drawn with.
const BANNER_WIDTH: usize = 108;

impl HistoryShow {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let rows = ctx
            .db
            .graph_history
            .series(
                &TimelineID(self.timeline_id.clone()),
                &self.node_name,
                &parse_bounds(self.start.as_deref(), self.end.as_deref())?,
                task,
            )
            .await?;
        ctx.println_after_done(&format_series(&rows))?;
        Ok(())
    }
}

fn format_series(rows: &[HistorySeriesRow]) -> String {
    let mut lines = vec![
        format!(
            "{:<12} {:<24} {:<12} {:<40} {}",
            "graph_id", "timestamp", "reasons", "metrics", "Δ vs prev frame"
        ),
        "-".repeat(BANNER_WIDTH),
    ];

    for (index, row) in rows.iter().enumerate() {
        let previous = index.checked_sub(1).map(|previous| &rows[previous]);
        if let Some(previous) = previous
            && !row.attributable
        {
            lines.push(unknown_region(previous, row));
        }
        lines.push(format!(
            "{:<12} {:<24} {:<12} {:<40} {}",
            row.graph_id.0,
            row.timestamp.to_comparable_rfc3339_str(),
            format_reasons(row.reasons),
            format_metrics(row),
            format_delta(row, previous),
        ));
    }
    lines.join("\n")
}

/// The banner drawn where two adjacent rows are not adjacent frames.
///
/// Loud on purpose. Reading straight across an unrecorded stretch is the single
/// easiest way to misattribute a change to the wrong diff, and the whole point
/// of this subsystem is to not do that.
fn unknown_region(previous: &HistorySeriesRow, row: &HistorySeriesRow) -> String {
    let frames = row.graph_id.0 - previous.graph_id.0 - 1;
    let cause = match previous.reasons.is_empty() || row.reasons.is_empty() {
        true => "no data",
        false => "nothing recorded",
    };
    let label = format!(
        " unknown region: {cause} for {frames} frame(s) between {} and {} ",
        previous.graph_id.0, row.graph_id.0
    );
    let rule = BANNER_WIDTH.saturating_sub(label.chars().count());
    format!(
        "{}{label}{}",
        "─".repeat(rule / 2),
        "─".repeat(rule - rule / 2)
    )
}

/// The row's reasons, shortest-first so the eye lands on the real data.
fn format_reasons(reasons: Reasons) -> String {
    if reasons.is_empty() {
        return "[gap-edge]".to_owned();
    }
    let tags = [
        (Reasons::OVER_THRESHOLD, "[CROSSING]"),
        (Reasons::ANCHOR, "[anchor]"),
        (Reasons::FIRST, "[first]"),
        (Reasons::LATEST, "[latest]"),
    ];
    tags.iter()
        .filter(|(flag, _)| reasons.contains(*flag))
        .map(|(_, tag)| *tag)
        .collect::<Vec<_>>()
        .join("")
}

fn format_metrics(row: &HistorySeriesRow) -> String {
    row.values
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What this row's own diff changed, where that is knowable.
fn format_delta(row: &HistorySeriesRow, previous: Option<&HistorySeriesRow>) -> String {
    let Some(previous) = previous.filter(|_| row.attributable) else {
        return "-".to_owned();
    };

    let changed = row
        .values
        .keys()
        .chain(previous.values.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|name| {
            let delta =
                row.values.get(name).unwrap_or(&0.0) - previous.values.get(name).unwrap_or(&0.0);
            (delta != 0.0).then(|| format!("{name} {delta:+}"))
        })
        .collect::<Vec<_>>();

    match changed.is_empty() {
        true => "no change".to_owned(),
        false => changed.join(", "),
    }
}

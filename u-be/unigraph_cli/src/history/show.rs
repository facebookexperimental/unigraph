// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;

use clap::Parser;
use unigraph_db::HistorySeriesRow;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;
use crate::history::compact::parse_bounds;

/// Print one node's kept history samples, with metric names resolved.
///
/// # Reading the delta column
///
/// A sample's metrics are absolute, so the gap between two printed rows is the
/// drift accumulated over every frame between them — which for a compacted
/// series can be hundreds of diffs, not the work of the graph on the row.
///
/// `Δ vs prev frame` is the honest attribution: the change against the frame
/// immediately before, which is printed exactly when the preceding row is that
/// frame — an `(anchor)` row, kept by ingest for this purpose. A `-` means the
/// preceding frame has no row, so this graph's own contribution is unknown.
///
/// ```sh
/// unigraph history show --timeline-id my_timeline --node-name my_node
/// ```
///
/// ```text
/// graph_id     timestamp                metrics      Δ vs prev frame
/// 1            2026-08-01T00:00:00Z     size=1       -
/// 999          2026-08-05T09:00:00Z     size=95      (anchor)
/// 1000         2026-08-05T10:00:00Z     size=100     size +5
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
    let mut lines = vec![format!(
        "{:<12} {:<24} {:<40} {}",
        "graph_id", "timestamp", "metrics", "Δ vs prev frame"
    )];
    lines.push("-".repeat(100));
    for (index, row) in rows.iter().enumerate() {
        lines.push(format!(
            "{:<12} {:<24} {:<40} {}",
            row.graph_id.0,
            row.timestamp.to_comparable_rfc3339_str(),
            format_metrics(row),
            format_delta(row, index.checked_sub(1).map(|prev| &rows[prev])),
        ));
    }
    lines.join("\n")
}

fn format_metrics(row: &HistorySeriesRow) -> String {
    row.values
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What this row's own graph changed, where that is knowable.
///
/// Only an anchor is guaranteed to be the row's immediate frame predecessor —
/// that is what an anchor is. Any other preceding row may be arbitrarily far
/// back, and differencing against it would attribute all the intervening drift
/// to this one graph, which is the whole thing this column exists to avoid.
fn format_delta(row: &HistorySeriesRow, previous: Option<&HistorySeriesRow>) -> String {
    if row.anchor {
        return "(anchor)".to_owned();
    }
    let Some(previous) = previous.filter(|previous| previous.anchor) else {
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;
use crate::history::compact::parse_bounds;

/// Print one node's kept history samples, with metric names resolved.
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

fn format_series(rows: &[unigraph_db::HistorySeriesRow]) -> String {
    let mut lines = vec![format!(
        "{:<12} {:<24} {}",
        "graph_id", "timestamp", "metrics"
    )];
    lines.push("-".repeat(80));
    for row in rows {
        let metrics = row
            .values
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "{:<12} {:<24} {}",
            row.graph_id.0,
            row.timestamp.to_comparable_rfc3339_str(),
            metrics
        ));
    }
    lines.join("\n")
}

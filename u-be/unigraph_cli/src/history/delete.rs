// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Delete recorded history entries and ingest checkpoints.
///
/// With no bounds this wipes the whole timeline's history (including its
/// metric-name dictionary), deleting in bounded graph-ID chunks so no single
/// transaction has to cover what can be a very large number of rows. Partial
/// progress is safe and the command is re-runnable.
///
/// Error blobs are only registered for cleanup here; `blob_storage.sweep`
/// removes them once they are past its `min_age` window.
#[derive(Parser, Debug)]
pub struct HistoryDelete {
    /// Timeline to delete history for
    #[arg(long)]
    timeline_id: String,

    /// Inclusive lower graph ID bound. Defaults to unbounded.
    #[arg(long)]
    from_graph_id: Option<i64>,

    /// Inclusive upper graph ID bound. Defaults to unbounded.
    #[arg(long)]
    to_graph_id: Option<i64>,
}

impl HistoryDelete {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let report = ctx
            .db
            .graph_history
            .delete(
                &TimelineID(self.timeline_id.clone()),
                &(
                    self.from_graph_id.map(GraphID),
                    self.to_graph_id.map(GraphID),
                ),
                task,
            )
            .await?;
        ctx.println_after_done(&format!(
            "deleted entries={} statuses={} metrics={} registered_error_blobs={}",
            report.entries_deleted,
            report.statuses_deleted,
            report.metrics_deleted,
            report.error_blobs_registered,
        ))?;
        Ok(())
    }
}

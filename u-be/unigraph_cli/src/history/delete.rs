// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Delete recorded history entries and ingest checkpoints.
///
/// With no bounds this wipes the whole timeline's history — every entry, every
/// checkpoint, and the metric-name dictionary — deleting in bounded graph-ID
/// chunks so no single transaction has to cover what can be a very large
/// number of rows. Partial progress is safe and the command is re-runnable.
///
/// Scoped to `--timeline-id` and nothing else. Graph IDs are allocated per
/// timeline, so every timeline's history occupies the same `graph_id` space
/// this walks; the `timeline_id` predicate on each statement is the only thing
/// keeping them apart, and it is on all of them.
///
/// The timeline itself and its frames are untouched — this only clears the
/// `graph_history_*` tables. Re-running `history ingest` rebuilds the series
/// from the frames, at whatever `--threshold` you give it.
///
/// Error blobs are only registered for cleanup here; `blob_storage.sweep`
/// removes them once they are past its `min_age` window.
///
/// An unbounded wipe requires `--yes`. A bounded delete does not — that is the
/// repair path, and the bounds are the statement of intent.
///
/// ```sh
/// # Wipe one timeline's history entirely
/// unigraph history delete --timeline-id my_timeline --yes
///
/// # Or just a graph-ID range, to repair part of a series
/// unigraph history delete --timeline-id my_timeline --from-graph-id 100 --to-graph-id 200
/// ```
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

    /// Confirm an unbounded wipe. Required when no graph-ID bound is given.
    #[arg(long)]
    yes: bool,
}

impl HistoryDelete {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let bounds = (
            self.from_graph_id.map(GraphID),
            self.to_graph_id.map(GraphID),
        );
        self.ensure_confirmed(&bounds)?;

        let report = ctx
            .db
            .graph_history
            .delete(&TimelineID(self.timeline_id.clone()), &bounds, task)
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

    /// Refuse an unbounded wipe that wasn't asked for explicitly.
    ///
    /// The timeline is named by a bare string, so a typo names a *different*
    /// real timeline rather than failing, and there is no dry run to catch it —
    /// the first sign of a mistake is a series that is gone. Nothing here is
    /// recoverable from history itself; re-ingesting from the frames is the
    /// only way back, and only for what the frames still hold.
    ///
    /// Bounded deletes are left unguarded on purpose. They are the repair path,
    /// they run often, and the bounds are already a statement of intent.
    ///
    /// A flag rather than a prompt: this command runs under the `meta` CLI and
    /// from scheduled jobs, where there is no one to answer a question.
    fn ensure_confirmed(&self, bounds: &GraphIDBounds) -> anyhow::Result<()> {
        if self.yes || bounds.0.is_some() || bounds.1.is_some() {
            return Ok(());
        }
        anyhow::bail!(
            "refusing to wipe all history for timeline '{}' without --yes.\n\
             This drops every entry, every ingest checkpoint and the metric-name \
             dictionary for that timeline. It cannot be undone — re-running \
             `history ingest` is the only way back, and only for frames that \
             still exist.\n\
             Pass --yes to confirm, or bound the delete with --from-graph-id / \
             --to-graph-id.",
            self.timeline_id,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a real argv so the test covers the flag wiring, not just the
    /// predicate underneath it.
    fn confirm(args: &[&str]) -> anyhow::Result<()> {
        let command =
            HistoryDelete::try_parse_from(std::iter::once("delete").chain(args.iter().copied()))?;
        command.ensure_confirmed(&(
            command.from_graph_id.map(GraphID),
            command.to_graph_id.map(GraphID),
        ))
    }

    #[test]
    fn only_an_unconfirmed_unbounded_wipe_is_refused() {
        let cases = [
            // (args, allowed, why)
            (
                vec!["--timeline-id", "t"],
                false,
                "an unbounded wipe needs --yes",
            ),
            (
                vec!["--timeline-id", "t", "--yes"],
                true,
                "--yes is the confirmation",
            ),
            (
                vec!["--timeline-id", "t", "--from-graph-id", "10"],
                true,
                "a lower bound scopes the delete, so it needs no confirmation",
            ),
            (
                vec!["--timeline-id", "t", "--to-graph-id", "10"],
                true,
                "an upper bound scopes it just as well",
            ),
            (
                vec![
                    "--timeline-id",
                    "t",
                    "--from-graph-id",
                    "1",
                    "--to-graph-id",
                    "9",
                ],
                true,
                "a fully bounded delete is the repair path",
            ),
        ];

        for (args, allowed, why) in cases {
            assert_eq!(confirm(&args).is_ok(), allowed, "{why}: {args:?}");
        }
    }

    #[test]
    fn the_refusal_says_how_to_proceed() {
        let Err(err) = confirm(&["--timeline-id", "www-budget"]) else {
            panic!("an unbounded wipe without --yes must be refused");
        };
        let message = format!("{err:#}");
        assert!(
            message.contains("www-budget"),
            "Name the timeline, so a typo is visible in the refusal: {message}"
        );
        assert!(
            message.contains("--yes"),
            "Say which flag unblocks it: {message}"
        );
        // The message is assembled with `\`-continuations, where dropping the
        // space before the backslash silently welds two words together.
        assert!(
            message.contains("the metric-name dictionary for that timeline"),
            "The refusal should read as prose, not run words together: {message}"
        );
    }
}

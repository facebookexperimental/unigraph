// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::io::IsTerminal;
use std::io::Write;

use clap::Parser;
use unigraph_db::DEFAULT_DELETE_BATCH_SIZE;
use unigraph_db::DEFAULT_DELETE_SWEEP_MIN_AGE;
use unigraph_db::TimelineDeleteOptions;
use unigraph_db::TimelineDeleteReport;
use unigraph_storage_core::TimelineID;

use crate::UnigraphCLIContext;

/// Delete a timeline and everything stored under it.
///
/// Removes every frame, all recorded history, the weekly metric history, the
/// external ID mappings for the timeline's namespace, and finally the timeline
/// itself. Frames go in transactions of `--batch-size` so a large timeline
/// never lands on the database as one enormous `DELETE`; partial progress is
/// safe and the command is re-runnable.
///
/// Traversal and graph-query configs are left alone — they are keyed by content
/// hash rather than by timeline, are shared between timelines, and expire on
/// their own TTL.
///
/// External blobs are *registered* for cleanup as their frames are deleted, and
/// the run ends by sweeping whatever in the cleanup queue has aged past the
/// deferral window. That window is deliberately not exposed as a flag: the
/// cleanup queue is shared across every timeline, and a store in flight
/// registers its blob before uploading it, so a shortened window can physically
/// delete a blob some *other* timeline's committing transaction is about to
/// reference. This run's own registrations are therefore left to the next
/// sweep, whether that is the next `blob_storage` maintenance pass or the
/// piggybacked sweep on any later frame delete.
///
/// There is no undo. Confirmation is interactive when stdin is a terminal
/// (type the timeline ID back); pass `--yes` for scripts and scheduled jobs.
///
/// ```sh
/// unigraph timelines delete my_timeline
/// unigraph timelines delete my_timeline --yes --batch-size 2000
/// ```
#[derive(Parser, Debug)]
pub struct TimelinesDelete {
    /// Timeline ID to delete
    timeline_id: String,

    /// Skip the confirmation prompt. Required when stdin is not a terminal.
    #[arg(long)]
    yes: bool,

    /// Frames to delete per transaction.
    #[arg(long, default_value_t = DEFAULT_DELETE_BATCH_SIZE)]
    batch_size: i64,
}

impl TimelinesDelete {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        self.confirm()?;

        // The sweep's deferral window is not a knob. It protects blobs belonging
        // to *other* timelines' in-flight stores, so no caller of this command
        // is in a position to know it is safe to shorten.
        let options = TimelineDeleteOptions {
            batch_size: self.batch_size,
            sweep_min_age: DEFAULT_DELETE_SWEEP_MIN_AGE,
        };
        let report = ctx
            .db
            .timelines
            .delete(&TimelineID(self.timeline_id.clone()), &options, task)
            .await?;

        ctx.println_after_done(&format_report(&self.timeline_id, &report))?;
        Ok(())
    }

    /// Refuse to delete a timeline nobody has confirmed.
    ///
    /// The timeline is named by a bare string, so a typo names a *different*
    /// real timeline rather than failing, and there is no dry run to catch it —
    /// the first sign of a mistake is a timeline that is gone, with no way back
    /// short of re-ingesting from the source. So the interactive path asks for
    /// the ID to be typed again rather than for a y/n: the thing worth
    /// confirming is *which* timeline, and a yes/no prompt confirms the typo
    /// along with everything else.
    ///
    /// `--yes` covers the non-interactive callers — this command also runs
    /// under the `meta` CLI and from scheduled jobs, where there is nobody to
    /// answer a question.
    fn confirm(&self) -> anyhow::Result<()> {
        if self.yes {
            return Ok(());
        }
        anyhow::ensure!(
            std::io::stdin().is_terminal(),
            "refusing to delete timeline '{}' without confirmation.\n\
             This deletes every frame, all recorded history, the metric history \
             and the external ID mappings for that timeline, then the timeline \
             itself. It cannot be undone.\n\
             stdin is not a terminal, so there is nobody to prompt: pass --yes \
             to confirm.",
            self.timeline_id,
        );
        self.prompt_for_timeline_id()
    }

    /// Ask for the timeline ID on the terminal and check what comes back.
    ///
    /// The live task tree is hiding while this runs — it repaints stderr every
    /// 50ms and would otherwise scribble over the prompt as it is being typed.
    fn prompt_for_timeline_id(&self) -> anyhow::Result<()> {
        ll_stdio::term_status::hide();
        let typed = read_confirmation(&self.timeline_id);
        ll_stdio::term_status::show();

        let typed = typed?;
        anyhow::ensure!(
            typed == self.timeline_id,
            "expected '{}' but got '{}' — nothing was deleted",
            self.timeline_id,
            typed,
        );
        Ok(())
    }
}

fn read_confirmation(timeline_id: &str) -> anyhow::Result<String> {
    eprint!(
        "About to permanently delete timeline '{timeline_id}' and everything \
         stored under it.\nType the timeline ID to confirm: "
    );
    std::io::stderr().flush()?;

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
}

/// One line per thing removed, plus the blob caveat, which is the only part of
/// the result that is not already final when the command returns.
fn format_report(timeline_id: &str, report: &TimelineDeleteReport) -> String {
    let mut lines = vec![
        format!("deleted timeline '{timeline_id}'"),
        format!(
            "  frames                {} (in {} batches)",
            report.frames_deleted, report.frame_batches
        ),
        format!(
            "  history               entries={} statuses={} metrics={}",
            report.history.entries_deleted,
            report.history.statuses_deleted,
            report.history.metrics_deleted,
        ),
        format!("  metric history        {}", report.metric_history_deleted),
        format!("  external id mappings  {}", report.external_ids_deleted),
    ];

    if let Some(other) = &report.external_id_namespace_shared_with {
        lines.push(format!(
            "  note: external ID mappings kept — timeline '{}' shares the namespace",
            other.0
        ));
    }

    let registered = report.blobs_registered + report.history.error_blobs_registered as u64;
    lines.push(format!(
        "  blobs                 {registered} registered for cleanup, {} swept now",
        report.blobs_swept
    ));
    lines.push(format!(
        "  (a blob is only swept once it has been pending for {}h; the rest go on the next sweep)",
        DEFAULT_DELETE_SWEEP_MIN_AGE.as_secs() / 3600,
    ));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a real argv so the test covers the flag wiring, not just the
    /// predicate underneath it.
    fn parse(args: &[&str]) -> TimelinesDelete {
        TimelinesDelete::try_parse_from(std::iter::once("delete").chain(args.iter().copied()))
            .expect("args should parse")
    }

    #[test]
    fn yes_is_the_only_thing_that_skips_the_prompt() {
        assert!(
            parse(&["t", "--yes"]).confirm().is_ok(),
            "--yes is the non-interactive confirmation"
        );
    }

    /// Under `cargo test` stdin is not a terminal, so this exercises the
    /// non-interactive refusal — the path a scheduled job would hit.
    #[test]
    fn a_non_interactive_run_without_yes_is_refused() {
        let Err(err) = parse(&["www-budget"]).confirm() else {
            panic!("a delete without --yes and without a terminal must be refused");
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
            message.contains("for that timeline"),
            "The refusal should read as prose, not run words together: {message}"
        );
    }

    #[test]
    fn the_batch_size_default_is_the_shared_one() {
        assert_eq!(parse(&["t"]).batch_size, DEFAULT_DELETE_BATCH_SIZE);
    }

    /// The sweep's deferral window protects blobs belonging to *other*
    /// timelines' in-flight stores, so no operator running this command is in a
    /// position to know it is safe to shorten. Deliberately not a flag — this
    /// guards against it being added back as a convenience.
    #[test]
    fn there_is_no_flag_for_the_sweep_window() {
        for flag in ["--sweep-min-age-secs", "--sweep-min-age", "--min-age"] {
            assert!(
                TimelinesDelete::try_parse_from(["delete", "t", flag, "0"]).is_err(),
                "{flag} must not be accepted"
            );
        }
    }

    #[test]
    fn the_report_flags_a_namespace_it_did_not_touch() {
        let report = TimelineDeleteReport {
            external_id_namespace_shared_with: Some(TimelineID("sibling".to_owned())),
            ..Default::default()
        };
        let text = format_report("mine", &report);
        assert!(
            text.contains("timeline 'sibling' shares the namespace"),
            "A skipped namespace has to be visible in the output: {text}"
        );
    }
}

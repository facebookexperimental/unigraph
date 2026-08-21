// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Running totals and rates for the long loops, rendered as task names.
//!
//! These loops can run for a long time over a number of rows nobody knows up
//! front, and `task.progress` can only carry `done/total` — for a delete that
//! is graph-ID *chunks*, which says little, since the ID space is sparse and a
//! chunk may cover ten thousand rows or none. The rate is what tells you
//! whether it is moving.
//!
//! A task's **name** is the only text the terminal reporter puts on screen
//! while work is in flight: it draws the task tree and the progress bar, and
//! does not render `task.data` at all (that reaches the log reporter, at task
//! end). So each step runs as a child task labelled with the totals so far.
//! Finished tasks leave the tree, so exactly one of these is on screen at a
//! time, under the parent's bar:
//!
//! ```text
//! ⠹ 12.4s delete_all_history ⣿⣿⣿⣀⣀ 7/23
//!   ⠹ 0.4s chunk 8/23 · 84.1k rows deleted · 21.3k/s
//! ```

use std::time::Instant;

/// How many items one progress window covers.
///
/// Only affects how often the on-screen label refreshes — each window is one
/// child task, so this trades reporter chatter against how stale the rate can
/// look. Small enough that a stalled window is obvious, large enough that the
/// task tree isn't doing more work than the loop it is describing.
pub(crate) const PROGRESS_WINDOW: usize = 100;

pub(crate) struct Throughput {
    started: Instant,
    pub(crate) done: u64,
}

impl Throughput {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            done: 0,
        }
    }

    pub(crate) fn add(&mut self, done: u64) {
        self.done += done;
    }

    /// `"chunk 8/23 · 84.1k rows deleted · 21.3k/s"`, as of the last step.
    ///
    /// The rate is measured over the whole loop rather than the last step:
    /// per-step rates on work this uneven swing by orders of magnitude between
    /// a chunk that hits a dense range and one that hits a gap.
    pub(crate) fn label(&self, step: i64, total: i64, unit: &str) -> String {
        let elapsed = self.started.elapsed().as_secs_f64();
        let rate = match elapsed > 0.0 {
            true => format!("{}/s", compact_count((self.done as f64 / elapsed) as u64)),
            false => "–/s".to_owned(),
        };
        format!(
            "chunk {step}/{total} · {} {unit} · {rate}",
            compact_count(self.done)
        )
    }
}

/// How many `chunk`-sized chunks `total` items span, rounding up.
///
/// `i64::div_ceil` is still unstable, and a partial trailing chunk has to
/// count or a loop reports a step past its own total on the last one.
pub(crate) fn chunks(total: i64, chunk: i64) -> i64 {
    let chunk = chunk.max(1);
    total.div_euclid(chunk) + i64::from(total % chunk != 0)
}

/// How many [`PROGRESS_WINDOW`]-sized windows `total` items span.
pub(crate) fn windows(total: i64) -> i64 {
    chunks(total, i64::try_from(PROGRESS_WINDOW).unwrap_or(1))
}

/// `9_999` → `"9999"`, `84_102` → `"84.1k"`, `2_400_000` → `"2.4M"`.
fn compact_count(value: u64) -> String {
    match value {
        0..=9_999 => value.to_string(),
        10_000..=999_999 => format!("{:.1}k", value as f64 / 1_000.0),
        _ => format!("{:.1}M", value as f64 / 1_000_000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_stay_short_enough_to_sit_in_a_task_name() {
        let cases = [
            (0, "0"),
            (9_999, "9999"),
            (10_000, "10.0k"),
            (84_102, "84.1k"),
            (999_999, "1000.0k"),
            (1_000_000, "1.0M"),
            (2_400_000, "2.4M"),
        ];
        for (value, expected) in cases {
            assert_eq!(compact_count(value), expected, "formatting {value}");
        }
    }

    /// A partial trailing window still counts, or the label would report a
    /// step past its own total on the last one.
    #[test]
    fn window_count_rounds_up() {
        let window = i64::try_from(PROGRESS_WINDOW).expect("the window fits in an i64");
        let cases = [
            (0, 0),
            (1, 1),
            (window, 1),
            (window + 1, 2),
            (window * 3, 3),
            (window * 3 + 1, 4),
        ];
        for (total, expected) in cases {
            assert_eq!(windows(total), expected, "windows for {total} items");
        }
    }

    #[test]
    fn chunk_count_rounds_up_and_survives_a_nonsense_chunk_size() {
        let cases = [
            // (total, chunk, expected, why)
            (0, 10, 0, "nothing to do is zero chunks, not one"),
            (1, 10, 1, "a partial chunk still counts"),
            (10, 10, 1, "an exact fit is one chunk"),
            (11, 10, 2, "one over spills into a second"),
            (
                67_524,
                10_000,
                7,
                "a real timeline at the default batch size",
            ),
            (5, 0, 5, "a zero chunk size must not divide by zero"),
            (5, -3, 5, "nor must a negative one"),
        ];
        for (total, chunk, expected, why) in cases {
            assert_eq!(chunks(total, chunk), expected, "{why}: {total}/{chunk}");
        }
    }

    #[test]
    fn the_label_reads_as_a_status_line() {
        let mut rate = Throughput::new();
        rate.add(84_102);
        let label = rate.label(8, 23, "rows deleted");

        assert!(
            label.starts_with("chunk 8/23 · 84.1k rows deleted · "),
            "unexpected label: {label}"
        );
        assert!(
            label.ends_with("/s"),
            "the label should end in a rate: {label}"
        );
    }
}

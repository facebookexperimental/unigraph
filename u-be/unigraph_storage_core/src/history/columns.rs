// Copyright (c) Meta Platforms, Inc. and affiliates.

//! The three column types the `graph_history_*` tables need beyond primitives.
//!
//! They live at the storage layer, next to the row structs that carry them, for
//! the same reason [`FrameType`](crate::types::FrameType) does: a column whose
//! legal values are a fixed set should be that set everywhere, including in the
//! trait the backends implement. The alternative — passing `u32` and `String`
//! across the boundary and re-parsing at every read site — puts the meaning in
//! the caller's head instead of the type, and that is exactly where the old
//! `deferred`/`anchor` pair went wrong.
//!
//! What lives here is the **vocabulary**: which flags exist, what they mean, and
//! how they survive a round trip through SQL. The **rules** that decide when to
//! set them — thresholds, gap structure, compaction — live in
//! `unigraph_db::graph_history`, which is where they can see a graph.

use std::fmt;
use std::str::FromStr;

use anyhow::Result;

bitflags::bitflags! {
    /// Why a stored history row exists — the set of independent justifications
    /// for it, OR'd together.
    ///
    /// A set rather than an enum, because a row is routinely more than one of
    /// these at once. The predecessor of this design had a single `anchor` flag
    /// whose meaning was "this row is *not* a threshold crossing", which made
    /// the two mutually exclusive by construction — wrong in a case that
    /// happens constantly: when a diff stack lands, consecutive frames each
    /// cross the threshold, and each one is simultaneously a crossing in its
    /// own right *and* the row that explains the crossing after it.
    ///
    /// An empty set means the row is collapsed — it holds nothing the series
    /// needs, and `history compact` will reclaim it unless the frame it sits on
    /// is a barrier (see [`FrameFlags`]).
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Reasons: u32 {
        /// The node's first recorded sample on this timeline.
        ///
        /// Kept unconditionally: there is nothing behind it to measure against,
        /// and without it the series would start at whatever frame the node
        /// first happened to move on, which reads as if the node sprang into
        /// existence there.
        const FIRST = 1 << 0;
        /// The node moved by at least the threshold against the immediately
        /// preceding *built* frame.
        ///
        /// The only reason that makes a row a baseline — see
        /// [`Reasons::is_baseline`]. This is the real data: one diff, one
        /// attributable movement.
        const OVER_THRESHOLD = 1 << 1;
        /// The next built frame keeps a crossing for this node, and this row is
        /// what makes that crossing's step readable.
        ///
        /// Without it the crossing's step reads as the whole drift since the
        /// last kept row — potentially hundreds of diffs — rather than the
        /// contribution of the one graph that crossed.
        const ANCHOR = 1 << 2;
        /// This is the newest built frame, so the row is the node's current
        /// value.
        ///
        /// Threshold decisions are made against the immediately preceding
        /// frame, which means a node that creeps upward by less than the
        /// threshold every frame records nothing at all. Pinning the newest
        /// frame bounds the resulting error to "between the last crossing and
        /// now" and makes the right-hand edge of a chart the truth rather than
        /// a stale absolute. Costs one row per node, always.
        const LATEST = 1 << 3;
    }
}

impl Reasons {
    /// Reasons that a change of threshold recomputes from scratch.
    ///
    /// [`Reasons::OVER_THRESHOLD`] and [`Reasons::ANCHOR`] are both answers to
    /// "how big is the step here". The other two answer "where does this row
    /// sit in the series", which no threshold can change — so compaction clears
    /// these two and preserves whatever else the row carries.
    pub const THRESHOLD_DERIVED: Reasons = Reasons::OVER_THRESHOLD.union(Reasons::ANCHOR);

    /// May a later sample's threshold decision be measured against this row?
    ///
    /// Only a crossing. An anchor sits within a threshold of the sample after
    /// it by construction, and a barrier row was kept without being judged at
    /// all, so measuring against either would hide drift that a real crossing
    /// should have recorded — and an omission cannot be undone.
    ///
    /// Note this is a property of the *reasons*, not of the row's identity: a
    /// row that is both a crossing and an anchor is a perfectly good baseline.
    pub const fn is_baseline(self) -> bool {
        self.contains(Reasons::OVER_THRESHOLD)
    }
}

bitflags::bitflags! {
    /// One frame's place in the gap structure.
    ///
    /// Stored per frame rather than per row, because gap structure is a
    /// property of the frame sequence alone: when a gap fills, that is two row
    /// writes instead of `2 x node_count`.
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct FrameFlags: u32 {
        /// The frame carries no metric values — it is part of a gap.
        const NO_DATA = 1 << 0;
        /// The frame has data and the frame before it does not.
        const AFTER_GAP = 1 << 1;
        /// The frame has data and the frame after it does not.
        const BEFORE_GAP = 1 << 2;
    }
}

impl FrameFlags {
    /// Flags that make a frame a barrier, and so hold every one of its rows
    /// regardless of what their reasons say.
    pub const BARRIER: FrameFlags = FrameFlags::AFTER_GAP.union(FrameFlags::BEFORE_GAP);

    pub const fn is_barrier(self) -> bool {
        self.intersects(FrameFlags::BARRIER)
    }
}

/// What history has done with one frame.
///
/// Also the work list: every frame whose state is not [`IngestState::Ingested`]
/// stays on it, with no time bound. That is what makes an ingest outage a delay
/// rather than a permanent hole — the design this replaced only ever looked at
/// a lookback window, so a frame that fell out of it was never reconsidered and
/// froze compaction behind it for good.
///
/// # The naming trap this fixes
///
/// The state this replaces called itself `Error` while
/// [`FrameType`](crate::types::FrameType) also has an `Error` — one meaning
/// "history failed to ingest this frame", the other "the source pipeline failed
/// to build it". Two unrelated failures, one word. Hence [`IngestState::Failed`]
/// for our own failure and [`IngestState::NoData`] for "there is nothing here".
///
/// There is deliberately no Processed/Omitted distinction. The old checkpoint
/// recorded whether a frame's own verdict kept any rows, which was actively
/// misleading: a frame checkpointed `Omitted` could still hold rows, written
/// after the fact by the frame *after* it to anchor a crossing. Whether a frame
/// has rows is a question for the entries table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IngestState {
    /// Known to history, not yet judged.
    ///
    /// Also how a frame is handed *back* to ingest: a frame that has stopped
    /// being the far edge of a gap has rows that were never judged, and setting
    /// it back to `Pending` is how it gets judged against its new neighbour
    /// without a second work-list mechanism.
    Pending,
    /// Judged. Its rows and their reasons are final for the threshold they were
    /// taken at.
    Ingested,
    /// The frame carries no metric values — `Empty` or `Error`.
    ///
    /// Stays on the work list forever: an `Empty` placeholder is exactly the
    /// thing that later becomes a real frame, and an `Error` frame can be
    /// rebuilt by the source pipeline. Re-listing costs nothing, because the
    /// frame is recognised from its type without fetching a graph.
    NoData,
    /// History could not read the frame. Carries `attempts`, and a blob key
    /// pointing at the last failure.
    Failed,
}

/// Stored forms. Renaming one is a data migration.
const STATE_PENDING: &str = "Pending";
const STATE_INGESTED: &str = "Ingested";
const STATE_NO_DATA: &str = "NoData";
const STATE_FAILED: &str = "Failed";

impl IngestState {
    /// Should ingest look at this frame again?
    ///
    /// `attempts` only matters for [`IngestState::Failed`] — everything else is
    /// either done or cheap to re-examine. The cap itself is the caller's
    /// policy, not this type's.
    pub fn needs_ingest(self, attempts: i64, max_attempts: u32) -> bool {
        match self {
            IngestState::Pending | IngestState::NoData => true,
            IngestState::Ingested => false,
            IngestState::Failed => attempts < i64::from(max_attempts),
        }
    }
}

impl fmt::Display for IngestState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            IngestState::Pending => STATE_PENDING,
            IngestState::Ingested => STATE_INGESTED,
            IngestState::NoData => STATE_NO_DATA,
            IngestState::Failed => STATE_FAILED,
        })
    }
}

impl FromStr for IngestState {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            STATE_PENDING => Ok(IngestState::Pending),
            STATE_INGESTED => Ok(IngestState::Ingested),
            STATE_NO_DATA => Ok(IngestState::NoData),
            STATE_FAILED => Ok(IngestState::Failed),
            other => Err(anyhow::anyhow!("Unknown IngestState: {other}")),
        }
    }
}

// -- Rendering --
//
// `bitflags` derives `Debug` as `Reasons(FIRST | ANCHOR)`, which is not what a
// snapshot, a CLI table or a task log wants. These render the bare names, and
// go through `Formatter::pad` so `{:<22}` in an aligned diagnostic actually
// works — a hand-rolled `write!` chain silently ignores width.

impl fmt::Display for Reasons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&render_flags(
            self.iter_names(),
            unknown_bits(self.bits(), Reasons::all().bits()),
        ))
    }
}

impl fmt::Display for FrameFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&render_flags(
            self.iter_names(),
            unknown_bits(self.bits(), FrameFlags::all().bits()),
        ))
    }
}

/// Bits set in `stored` that this build has no name for.
const fn unknown_bits(stored: u32, known: u32) -> u32 {
    stored & !known
}

/// `FIRST|ANCHOR`, or `-` when empty.
///
/// Bits this build does not recognise are appended in hex rather than dropped.
/// They are kept on the way in too ([`Reasons::from_bits_retain`], never
/// `from_bits`) — the invariant that decides whether a row survives is "has any
/// reason at all", so silently masking an unknown one would make a row written
/// by a newer binary look collapsed and deletable.
fn render_flags<I, F>(names: I, unknown: u32) -> String
where
    I: Iterator<Item = (&'static str, F)>,
{
    let mut out = String::new();
    for (name, _) in names {
        if !out.is_empty() {
            out.push('|');
        }
        out.push_str(name);
    }
    match (out.is_empty(), unknown) {
        (true, 0) => out.push('-'),
        (_, 0) => {}
        (true, rest) => out.push_str(&format!("{rest:#x}")),
        (false, rest) => out.push_str(&format!("|{rest:#x}")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The combinations the old single `anchor` flag could not express, and
    /// what each one means.
    #[test]
    fn a_row_can_be_a_crossing_and_an_anchor_at_the_same_time() {
        let cases = [
            (Reasons::empty(), "collapsed — compaction reclaims it"),
            (Reasons::FIRST, "the node's first sample"),
            (Reasons::OVER_THRESHOLD, "an attributable movement"),
            (Reasons::ANCHOR, "explains the crossing after it"),
            (
                Reasons::OVER_THRESHOLD | Reasons::ANCHOR,
                "a diff stack: crosses, and explains the next crossing",
            ),
            (Reasons::FIRST | Reasons::LATEST, "a single-frame timeline"),
        ];

        let report = cases
            .iter()
            .map(|(reasons, why)| {
                format!(
                    "{reasons:<30} baseline {:<5}  deletable {:<5}  {why}",
                    reasons.is_baseline(),
                    reasons.is_empty(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        k9::snapshot!(
            report,
            "
-                              baseline false  deletable true   collapsed — compaction reclaims it
FIRST                          baseline false  deletable false  the node's first sample
OVER_THRESHOLD                 baseline true   deletable false  an attributable movement
ANCHOR                         baseline false  deletable false  explains the crossing after it
OVER_THRESHOLD|ANCHOR          baseline true   deletable false  a diff stack: crosses, and explains the next crossing
FIRST|LATEST                   baseline false  deletable false  a single-frame timeline
"
        );
    }

    /// An unrecognised bit must survive the round trip and keep the row alive.
    /// Masking it would make a newer binary's row look collapsed and deletable.
    #[test]
    fn unknown_bits_are_retained_not_masked() {
        let future = Reasons::from_bits_retain(1 << 20);

        assert!(!future.is_empty(), "an unknown reason still keeps the row");
        assert_eq!(Reasons::from_bits_retain(future.bits()), future);
        assert_eq!(
            Reasons::from_bits(1 << 20),
            None,
            "the checked constructor is the one that rejects it — we do not use it"
        );
    }

    /// Re-thresholding recomputes the step-shaped reasons and must leave the
    /// position-shaped ones exactly as they were.
    #[test]
    fn re_thresholding_preserves_only_the_position_reasons() {
        let stored = Reasons::FIRST | Reasons::OVER_THRESHOLD | Reasons::ANCHOR | Reasons::LATEST;

        assert_eq!(
            stored.difference(Reasons::THRESHOLD_DERIVED),
            Reasons::FIRST | Reasons::LATEST,
            "raising the threshold may retract a crossing and its anchor, but \
             it cannot make a row stop being the node's first sample or the \
             newest frame"
        );
    }

    /// Width and alignment have to reach the rendering, or every aligned
    /// diagnostic in this subsystem quietly loses its columns.
    #[test]
    fn flags_render_by_name_and_honour_padding() {
        assert_eq!(format!("[{:<10}]", Reasons::FIRST), "[FIRST     ]");
        assert_eq!(format!("[{:>10}]", Reasons::FIRST), "[     FIRST]");
        assert_eq!(format!("[{:<10}]", Reasons::empty()), "[-         ]");
        assert_eq!(FrameFlags::BARRIER.to_string(), "AFTER_GAP|BEFORE_GAP");
    }

    #[test]
    fn only_a_barrier_holds_its_rows_unconditionally() {
        assert!(FrameFlags::AFTER_GAP.is_barrier());
        assert!(FrameFlags::BEFORE_GAP.is_barrier());
        assert!(FrameFlags::BARRIER.is_barrier());
        assert!(!FrameFlags::NO_DATA.is_barrier());
        assert!(
            !FrameFlags::empty().is_barrier(),
            "an interior frame holds only the rows that earned it"
        );
    }

    /// The stored form is the string, so a variant rename is a data migration.
    #[test]
    fn state_round_trips_through_its_stored_string() {
        for state in [
            IngestState::Pending,
            IngestState::Ingested,
            IngestState::NoData,
            IngestState::Failed,
        ] {
            assert_eq!(
                state.to_string().parse::<IngestState>().expect("parses"),
                state
            );
        }
        assert!(
            "Processed".parse::<IngestState>().is_err(),
            "a state from the old schema must fail loudly, not decode as something else"
        );
    }

    #[test]
    fn only_ingested_and_an_exhausted_failure_leave_the_work_list() {
        const CAP: u32 = 5;
        let cases = [
            (IngestState::Pending, 0, true, "never judged"),
            (
                IngestState::Pending,
                99,
                true,
                "attempts do not bound a pending frame",
            ),
            (IngestState::Ingested, 0, false, "done"),
            (
                IngestState::NoData,
                0,
                true,
                "a placeholder may still be filled, with no time limit",
            ),
            (IngestState::Failed, 1, true, "retryable"),
            (
                IngestState::Failed,
                i64::from(CAP),
                false,
                "past the cap, so it stops burning a graph fetch per run",
            ),
        ];

        for (state, attempts, expected, why) in cases {
            assert_eq!(
                state.needs_ingest(attempts, CAP),
                expected,
                "{state} with {attempts} attempts: {why}"
            );
        }
    }
}

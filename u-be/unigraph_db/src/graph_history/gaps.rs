// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Where the timeline has no data, and what that costs at the edges.
//!
//! # A gap is a run of frames we cannot attribute anything to
//!
//! The source pipeline registers frames in `graph_id` order and builds them out
//! of order, so at any moment the timeline is pocked with holes. Some fill
//! minutes later, some fill days later, and most never fill at all — their
//! source counterpart failed to build. A frame that failed to *build* (`Error`)
//! and a frame that was never built (`Empty`) mean exactly the same thing to
//! this subsystem: **the diff landed, the code changed, and we do not know what
//! the metric did.** They are treated identically.
//!
//! ```text
//! frame   04    05    06    07    08    09
//! type    Full  Full  .     .     .     Full
//!                     └──── gap ────┘
//!               ^^^^                ^^^^
//!               BEFORE_GAP          AFTER_GAP
//! ```
//!
//! # Barriers
//!
//! The built frames on either side of a gap keep a row for **every** node,
//! unconditionally, whatever the threshold says. They bound the unknown region:
//! the value is known on each side, and the step across is explicitly not
//! attributable to any one diff. Without them a chart draws a straight line
//! over the hole and blames the next frame for everything that happened inside
//! it.
//!
//! Barriers are temporary. When the gap fills the reason evaporates, the rows
//! become ordinary collapse candidates, and the frame that used to sit after
//! the gap gets judged against its new neighbour.
//!
//! # Why this replaces the settled frontier
//!
//! The predecessor of this design had one global frontier: the highest
//! `graph_id` whose entire prefix had stopped changing. Compaction refused to
//! look past it, so a single unfilled frame near the head froze the whole
//! timeline behind it — on `www-budget` that stranded 8,597 frames and ~264,000
//! rows that nothing would ever reclaim.
//!
//! Barriers are local. Every stretch of frames between two of them is judgeable
//! on its own, whether or not any gap ever fills, so compaction runs on each
//! [`Segment`] independently and no hole can hold anything hostage but its own
//! two neighbours.

use unigraph_storage_core::ExclusiveGraphIDRange;
use unigraph_storage_core::FrameFlags;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::IngestState;

/// Does this frame carry metric values history can use?
///
/// `Empty` and `Error` never do. A built frame does — unless history itself
/// could not read it, which is just as much a hole in the record as an unbuilt
/// frame, and un-holes itself if a retry succeeds.
pub fn frame_has_data(frame_type: &FrameType, ingest_state: Option<IngestState>) -> bool {
    match frame_type {
        FrameType::Empty | FrameType::Error => false,
        FrameType::Full | FrameType::Delta => ingest_state != Some(IngestState::Failed),
    }
}

/// One frame, reduced to what the gap structure cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameGap {
    pub graph_id: GraphID,
    /// See [`frame_has_data`].
    pub has_data: bool,
    /// The flags currently stored for this frame.
    pub stored: FrameFlags,
}

/// A frame whose stored flags no longer match the frame sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagUpdate {
    pub graph_id: GraphID,
    /// What the flags should be.
    pub flags: FrameFlags,
    /// The frame has stopped being the first one after a gap.
    ///
    /// Its rows were written without ever being judged — that is what a barrier
    /// is — and now there is a neighbour to judge them against. Nothing here can
    /// do that: it needs the predecessor's values, which means a graph. The
    /// orchestrator hands such a frame back to `ingest`.
    pub needs_rejudge: bool,
}

/// A stretch of frames compaction may collapse within, bounded by barriers.
///
/// Bounds are **exclusive**: barrier rows are held by their frame flags, so
/// leaving them outside every segment means the collapse delete is a plain
/// single-table range statement with nothing to join against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// The barrier below, or `None` at the start of the timeline.
    pub after: Option<GraphID>,
    /// The barrier above, or `None` at the end of the timeline.
    pub before: Option<GraphID>,
}

/// What each frame's flags should be, given the sequence around it.
///
/// `frames` must be a contiguous window in ascending `graph_id`. The outermost
/// two are computed as if nothing lay beyond them, so a caller working on a
/// sub-range should include one frame of context on each side — or, better,
/// pass the whole timeline, which is a metadata-only read and cheap even at
/// tens of thousands of frames.
pub fn desired_flags(frames: &[FrameGap]) -> Vec<FrameFlags> {
    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let previous_is_gap = index
                .checked_sub(1)
                .is_some_and(|before| !frames[before].has_data);
            let next_is_gap = frames.get(index + 1).is_some_and(|after| !after.has_data);

            let mut flags = FrameFlags::empty();
            flags.set(FrameFlags::NO_DATA, !frame.has_data);
            flags.set(FrameFlags::AFTER_GAP, frame.has_data && previous_is_gap);
            flags.set(FrameFlags::BEFORE_GAP, frame.has_data && next_is_gap);
            flags
        })
        .collect()
}

/// The frames whose stored flags have gone stale, and which of them need their
/// rows judged again.
///
/// Idempotent by construction: a second call over an unchanged sequence returns
/// nothing.
pub fn reconcile_flags(frames: &[FrameGap]) -> Vec<FlagUpdate> {
    desired_flags(frames)
        .into_iter()
        .zip(frames)
        .filter(|(flags, frame)| *flags != frame.stored)
        .map(|(flags, frame)| FlagUpdate {
            graph_id: frame.graph_id,
            flags,
            needs_rejudge: frame.stored.contains(FrameFlags::AFTER_GAP)
                && !flags.contains(FrameFlags::AFTER_GAP),
        })
        .collect()
}

/// The stretches between barriers, in ascending order.
///
/// Always returns at least one segment: a timeline with no barriers at all is
/// one unbounded stretch.
pub fn segments(frames: &[FrameGap], flags: &[FrameFlags]) -> Vec<Segment> {
    let barriers = frames
        .iter()
        .zip(flags)
        .filter(|(_, flags)| flags.is_barrier())
        .map(|(frame, _)| frame.graph_id);

    let mut segments = Vec::new();
    let mut after = None;
    for barrier in barriers {
        segments.push(Segment {
            after,
            before: Some(barrier),
        });
        after = Some(barrier);
    }
    segments.push(Segment {
        after,
        before: None,
    });
    segments
}

/// An exclusive range covering exactly one frame.
///
/// Lets a caller reuse the segment-shaped collapse delete to clear whatever a
/// single frame held from an earlier judgement — `graph_id > n - 1 AND
/// graph_id < n + 1` selects `n` and nothing else, however sparse the ID space
/// is.
pub fn only_frame(graph_id: GraphID) -> ExclusiveGraphIDRange {
    ExclusiveGraphIDRange {
        after: Some(GraphID(graph_id.0.saturating_sub(1))),
        before: Some(GraphID(graph_id.0.saturating_add(1))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `F` has data, `.` does not — the same notation the end-to-end snapshots
    /// use.
    fn parse(shape: &str) -> Vec<FrameGap> {
        shape
            .chars()
            .enumerate()
            .map(|(index, symbol)| FrameGap {
                graph_id: GraphID(index as i64 + 1),
                has_data: symbol == 'F',
                stored: FrameFlags::empty(),
            })
            .collect()
    }

    fn render(frames: &[FrameGap]) -> String {
        desired_flags(frames)
            .iter()
            .map(
                |flags| match (flags.contains(FrameFlags::NO_DATA), *flags) {
                    (true, _) => '.',
                    (false, f) if f.contains(FrameFlags::BARRIER) => 'X',
                    (false, f) if f.contains(FrameFlags::AFTER_GAP) => '>',
                    (false, f) if f.contains(FrameFlags::BEFORE_GAP) => '<',
                    _ => 'o',
                },
            )
            .collect()
    }

    /// `o` interior   `<` before a gap   `>` after a gap   `X` both   `.` gap
    #[test]
    fn barriers_land_on_the_built_frames_bounding_every_gap() {
        let cases = [
            ("FFFFF", "the happy path: no gaps, no barriers"),
            ("...", "an all-gap window has nothing to bound"),
            ("F..F", "one hole, one barrier on each side"),
            ("F.F.F", "a lone frame between two holes is both"),
            (".FF.", "the window's own ends are not gaps"),
            ("FF..FF..FF", "two holes, four barriers"),
            ("F", "a single frame is bounded by nothing"),
            (".F.", "a single frame between two holes"),
        ];

        let report = cases
            .iter()
            .map(|(shape, why)| format!("{shape:<12} {:<12} {why}", render(&parse(shape))))
            .collect::<Vec<_>>()
            .join("\n");

        k9::snapshot!(
            report,
            "
FFFFF        ooooo        the happy path: no gaps, no barriers
...          ...          an all-gap window has nothing to bound
F..F         <..>         one hole, one barrier on each side
F.F.F        <.X.>        a lone frame between two holes is both
.FF.         .><.         the window's own ends are not gaps
FF..FF..FF   o<..><..>o   two holes, four barriers
F            o            a single frame is bounded by nothing
.F.          .X.          a single frame between two holes
"
        );
    }

    /// Segments are the open intervals between barriers, so a barrier's own
    /// rows are never inside one and never reachable by a collapse delete.
    #[test]
    fn segments_are_the_open_intervals_between_barriers() {
        let format = |shape: &str| {
            let frames = parse(shape);
            let flags = desired_flags(&frames);
            segments(&frames, &flags)
                .iter()
                .map(|segment| {
                    let bound = |id: Option<GraphID>| {
                        id.map_or_else(|| "-".to_owned(), |id| id.0.to_string())
                    };
                    format!("({}, {})", bound(segment.after), bound(segment.before))
                })
                .collect::<Vec<_>>()
                .join(" ")
        };

        let cases = ["FFFFF", "F..F", "FF..FF..FF", "..."];
        let report = cases
            .iter()
            .map(|shape| format!("{shape:<12} {}", format(shape)))
            .collect::<Vec<_>>()
            .join("\n");

        k9::snapshot!(
            report,
            "
FFFFF        (-, -)
F..F         (-, 1) (1, 4) (4, -)
FF..FF..FF   (-, 2) (2, 5) (5, 6) (6, 9) (9, -)
...          (-, -)
"
        );
    }

    /// The blast radius of a late fill: only the frames whose neighbourhood
    /// actually changed are rewritten, and only the one that stopped being a
    /// gap's far edge needs judging again.
    #[test]
    fn filling_a_gap_touches_three_frames_and_re_judges_one() {
        let before = parse("FF.FF");
        let stored = desired_flags(&before);
        let mut after = parse("FFFFF");
        for (frame, flags) in after.iter_mut().zip(&stored) {
            frame.stored = *flags;
        }

        let updates = reconcile_flags(&after);
        let report = updates
            .iter()
            .map(|update| {
                format!(
                    "graph {} -> {:<12} rejudge {}",
                    update.graph_id.0, update.flags, update.needs_rejudge
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        k9::snapshot!(
            report,
            "
graph 2 -> -            rejudge false
graph 3 -> -            rejudge false
graph 4 -> -            rejudge true
"
        );
        assert_eq!(
            reconcile_flags(&{
                let mut settled = after.clone();
                for (frame, flags) in settled.iter_mut().zip(desired_flags(&after)) {
                    frame.stored = flags;
                }
                settled
            }),
            vec![],
            "a second pass over an unchanged sequence must find nothing to do"
        );
    }
}

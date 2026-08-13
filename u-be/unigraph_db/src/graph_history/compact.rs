// Copyright (c) Meta Platforms, Inc. and affiliates.

//! What compaction should do to one node's stored rows.
//!
//! Two jobs, both pure:
//!
//! 1. **Collapse.** A row whose reasons have all evaporated holds no
//!    information the series needs. The usual way that happens is a gap
//!    filling: the barrier rows on either side were kept without being judged,
//!    and once the hole closes nothing is holding them any more.
//! 2. **Re-threshold.** Every crossing stores its anchor, so
//!    *"would this still cross at a higher threshold?"* is answerable from the
//!    rows alone, with no graph fetch.
//!
//! # Compaction can only raise the threshold
//!
//! *Lowering* it would need values that were never written. That is a
//! deliberate non-goal: re-ingest is the way to lower a threshold.
//!
//! # It never invents a row
//!
//! Where the row a decision needs is absent, the stored verdict stands. Two
//! shapes reach that state and only one of them is garbage, and since dropping
//! a row is exactly as irreversible as never writing it, the tie goes to
//! keeping:
//!
//! - a node that *appeared* at this frame has a crossing with nothing to anchor
//!   it, because there was no predecessor value to record;
//! - a stale row left behind by a barrier whose gap has closed has no reasons
//!   to begin with, so it is dropped anyway.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use unigraph_storage_core::GraphID;

use crate::graph_history::Reasons;
use crate::graph_history::threshold::Values;
use crate::graph_history::threshold::crosses;

/// One of a node's stored rows in the range being compacted.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactRow {
    pub graph_id: GraphID,
    pub values: Values,
    pub reasons: Reasons,
}

/// One node's stored series plus the frame context needed to judge it.
pub struct CompactInput<'a> {
    /// The node's rows in the range, ascending by `graph_id`.
    pub series: &'a [CompactRow],
    /// Every frame in the range that carries data, ascending.
    ///
    /// Which row explains which is a *frame* question, not a row question: a
    /// crossing's predecessor usually has no row of its own, and the row before
    /// it in `series` can be thousands of frames back.
    ///
    /// A bounded range cannot see the data frame below its lower bound, so the
    /// range's first row is judged conservatively — its stored verdict stands.
    /// Compacting the whole timeline, which is the default, has no such edge.
    pub frames: &'a [GraphID],
    /// Frames with a gap immediately behind them.
    ///
    /// Load-bearing, and easy to leave out: `frames` lists the frames that
    /// carry data, so two of them can be adjacent *in that list* while a run of
    /// unbuilt frames sits between them in the timeline. Measuring across that
    /// would attribute a whole unknown region to one diff — the exact mistake
    /// this subsystem exists to prevent — so a frame listed here is treated as
    /// having no predecessor at all.
    pub after_gap: &'a BTreeSet<GraphID>,
    /// Frames whose rows are held by their frame flags whatever their reasons
    /// say. See [`crate::graph_history::gaps`].
    pub barriers: &'a BTreeSet<GraphID>,
    pub threshold: f64,
}

/// What compaction should write and delete for one node.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CompactPlan {
    /// Rows whose reason set changed but that still have one. Rewrite them.
    pub updated: Vec<(GraphID, Reasons)>,
    /// Rows with no reason left and no barrier holding them. Delete them.
    pub dropped: Vec<GraphID>,
}

impl CompactPlan {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.dropped.is_empty()
    }
}

/// Decide the fate of every row in one node's series.
///
/// Idempotent: running it twice at the same threshold finds nothing to do the
/// second time.
pub fn compact_series(input: &CompactInput<'_>) -> CompactPlan {
    let stored = input
        .series
        .iter()
        .map(|row| (row.graph_id, row))
        .collect::<BTreeMap<_, _>>();

    let crossings = input
        .series
        .iter()
        .filter(|row| still_crosses(row, &stored, input))
        .map(|row| row.graph_id)
        .collect::<BTreeSet<_>>();
    let anchors = anchor_frames(&crossings, input, &stored);

    let mut plan = CompactPlan::default();
    for row in input.series {
        let mut reasons = row.reasons.difference(Reasons::THRESHOLD_DERIVED);
        reasons.set(Reasons::OVER_THRESHOLD, crossings.contains(&row.graph_id));
        reasons.set(Reasons::ANCHOR, anchors.contains(&row.graph_id));

        if reasons.is_empty() && !input.barriers.contains(&row.graph_id) {
            plan.dropped.push(row.graph_id);
        } else if reasons != row.reasons {
            plan.updated.push((row.graph_id, reasons));
        }
    }
    plan
}

/// Does this row still clear the threshold against the row at the data frame
/// immediately before it?
///
/// With no row there to measure against, the stored verdict stands — see the
/// module docs for why that is the safe direction.
fn still_crosses(
    row: &CompactRow,
    stored: &BTreeMap<GraphID, &CompactRow>,
    input: &CompactInput<'_>,
) -> bool {
    let Some(previous) = preceding_frame(row.graph_id, input) else {
        return row.reasons.contains(Reasons::OVER_THRESHOLD);
    };
    let Some(previous) = stored.get(&previous) else {
        return row.reasons.contains(Reasons::OVER_THRESHOLD);
    };
    crosses(Some(&previous.values), Some(&row.values), input.threshold)
}

/// The data frame immediately before `graph_id`, where one is adjacent.
///
/// `None` across a gap: the previous entry in `frames` is then the far side of
/// an unknown region, not a predecessor.
fn preceding_frame(graph_id: GraphID, input: &CompactInput<'_>) -> Option<GraphID> {
    if input.after_gap.contains(&graph_id) {
        return None;
    }
    let index = input.frames.binary_search(&graph_id).ok()?;
    input.frames.get(index.checked_sub(1)?).copied()
}

/// The rows that have to stay so each crossing's step reads as its own diff's
/// contribution.
fn anchor_frames(
    crossings: &BTreeSet<GraphID>,
    input: &CompactInput<'_>,
    stored: &BTreeMap<GraphID, &CompactRow>,
) -> BTreeSet<GraphID> {
    crossings
        .iter()
        .filter_map(|graph_id| preceding_frame(*graph_id, input))
        .filter(|graph_id| stored.contains_key(graph_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row per data frame, values as given, reasons as ingest would have left
    /// them at `threshold`.
    fn ingested(values: &[(i64, f64)], threshold: f64) -> Vec<CompactRow> {
        let mut rows = values
            .iter()
            .map(|(graph_id, value)| CompactRow {
                graph_id: GraphID(*graph_id),
                values: Values::from([(1, *value)]),
                reasons: Reasons::empty(),
            })
            .collect::<Vec<_>>();

        rows[0].reasons = Reasons::FIRST;
        for index in 1..rows.len() {
            let moved = crosses(
                Some(&rows[index - 1].values),
                Some(&rows[index].values),
                threshold,
            );
            if moved {
                rows[index].reasons |= Reasons::OVER_THRESHOLD;
                rows[index - 1].reasons |= Reasons::ANCHOR;
            }
        }
        if let Some(last) = rows.last_mut() {
            last.reasons |= Reasons::LATEST;
        }
        rows
    }

    fn frames(values: &[(i64, f64)]) -> Vec<GraphID> {
        values
            .iter()
            .map(|(graph_id, _)| GraphID(*graph_id))
            .collect()
    }

    fn apply(rows: Vec<CompactRow>, plan: &CompactPlan) -> Vec<CompactRow> {
        rows.into_iter()
            .filter(|row| !plan.dropped.contains(&row.graph_id))
            .map(|row| {
                let reasons = plan
                    .updated
                    .iter()
                    .find(|(graph_id, _)| *graph_id == row.graph_id)
                    .map_or(row.reasons, |(_, reasons)| *reasons);
                CompactRow { reasons, ..row }
            })
            .collect()
    }

    fn render(rows: &[CompactRow]) -> String {
        rows.iter()
            .map(|row| {
                format!(
                    "{:>3} {:<8} {}",
                    row.graph_id.0, row.values[&1], row.reasons
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn plan_for(rows: &[CompactRow], values: &[(i64, f64)], threshold: f64) -> CompactPlan {
        compact_series(&CompactInput {
            series: rows,
            frames: &frames(values),
            after_gap: &BTreeSet::new(),
            barriers: &BTreeSet::new(),
            threshold,
        })
    }

    /// The worked example from the redesign doc (III.9), end to end: values at
    /// frames 01..12 with a threshold of 3. This is the clearest statement of
    /// what the whole subsystem is for, so it is pinned here as well as in the
    /// storage tests.
    #[test]
    fn the_worked_example_from_the_design_doc() {
        let values = [
            (1, 10.0),
            (2, 10.0),
            (3, 15.0),
            (4, 15.0),
            (5, 15.0),
            (6, 20.0),
            (7, 20.0),
            (8, 21.0),
            (9, 22.0),
            (10, 23.0),
            (11, 24.0),
            (12, 29.0),
        ];
        let rows = ingested(&values, 3.0);
        let plan = plan_for(&rows, &values, 3.0);

        k9::snapshot!(
            render(&apply(rows, &plan)),
            "
  1 10       FIRST
  2 10       ANCHOR
  3 15       OVER_THRESHOLD
  5 15       ANCHOR
  6 20       OVER_THRESHOLD
 11 24       ANCHOR
 12 29       OVER_THRESHOLD|LATEST
"
        );
        assert_eq!(
            plan.updated,
            vec![],
            "ingest already wrote these reasons, so compaction only reclaims"
        );
    }

    /// Rows that explain nothing at the new threshold are gone after one pass,
    /// and a second pass has nothing left to do.
    #[test]
    fn re_thresholding_collapses_once_and_then_settles() {
        let values = [
            (1, 0.0),
            (2, 4.0),
            (3, 9.0),
            (4, 10.0),
            (5, 14.0),
            (6, 20.0),
        ];
        let rows = ingested(&values, 3.0);

        // Nothing steps by 10 or more from one frame to the next, so at that
        // threshold no diff is to blame for anything and only the two
        // position-shaped reasons survive.
        let plan = plan_for(&rows, &values, 10.0);
        let compacted = apply(rows, &plan);
        k9::snapshot!(
            render(&compacted),
            "
  1 0        FIRST
  6 20       LATEST
"
        );
        assert_eq!(
            plan_for(&compacted, &values, 10.0),
            CompactPlan::default(),
            "a second pass at the same threshold must be a no-op"
        );
    }

    /// Raising the threshold retracts a crossing, and the anchor that existed
    /// only to explain it goes with it.
    #[test]
    fn raising_the_threshold_retracts_a_crossing_and_its_anchor() {
        let values = [(1, 0.0), (2, 95.0), (3, 100.0)];
        let rows = ingested(&values, 3.0);
        k9::snapshot!(
            render(&rows),
            "
  1 0        FIRST|ANCHOR
  2 95       OVER_THRESHOLD|ANCHOR
  3 100      OVER_THRESHOLD|LATEST
"
        );

        // The old design could not represent frame 2's row: `anchor` meant
        // "not a crossing", so flagging it removed a real +95 from baseline
        // lookups.
        let plan = plan_for(&rows, &values, 50.0);
        k9::snapshot!(
            render(&apply(rows, &plan)),
            "
  1 0        FIRST|ANCHOR
  2 95       OVER_THRESHOLD
  3 100      LATEST
"
        );
    }

    /// A barrier holds its rows even with nothing else to justify them, and
    /// releases them the moment the gap closes.
    #[test]
    fn barrier_rows_survive_until_the_gap_closes() {
        let values = [(1, 10.0), (2, 10.0)];
        let rows = vec![
            CompactRow {
                graph_id: GraphID(1),
                values: Values::from([(1, 10.0)]),
                reasons: Reasons::empty(),
            },
            CompactRow {
                graph_id: GraphID(2),
                values: Values::from([(1, 10.0)]),
                reasons: Reasons::empty(),
            },
        ];

        let held = compact_series(&CompactInput {
            series: &rows,
            frames: &frames(&values),
            after_gap: &BTreeSet::new(),
            barriers: &BTreeSet::from([GraphID(1), GraphID(2)]),
            threshold: 3.0,
        });
        assert_eq!(
            held,
            CompactPlan::default(),
            "while the gap is open both rows bound an unknown region"
        );

        let released = plan_for(&rows, &values, 3.0);
        k9::snapshot!(
            format!("dropped {:?}", released.dropped),
            "dropped [GraphID(1), GraphID(2)]"
        );
    }

    /// A crossing whose predecessor row is absent cannot be re-judged, so it
    /// keeps the verdict it was ingested with. That is the node-appeared case,
    /// where there was no earlier value to anchor against.
    #[test]
    fn a_crossing_with_no_predecessor_row_keeps_its_verdict() {
        let values = [(1, 10.0), (5, 500.0)];
        let rows = vec![CompactRow {
            graph_id: GraphID(5),
            values: Values::from([(1, 500.0)]),
            reasons: Reasons::OVER_THRESHOLD,
        }];

        assert_eq!(
            plan_for(&rows, &values, 1_000_000.0),
            CompactPlan::default(),
            "with nothing to measure against, dropping the row would be a \
             guess — and an irreversible one"
        );
    }

    /// Compaction must not reach across a gap.
    ///
    /// `frames` lists the frames that *carry data*, so the two sides of a hole
    /// sit next to each other in it while a run of unbuilt frames separates
    /// them in the timeline. Measuring across that would credit one diff with a
    /// whole unknown region — and then anchor the far side to make the lie
    /// legible.
    #[test]
    fn a_gap_is_never_measured_across() {
        let values = [(1, 10.0), (2, 10.0), (5, 900.0), (6, 900.0)];
        // Frames 3 and 4 carry no data, so 5 sits after a gap and both 2 and 5
        // are barriers holding their rows. 1 and 6 carry the position reasons
        // any real series has at its ends.
        let reasons = [
            Reasons::FIRST,
            Reasons::empty(),
            Reasons::empty(),
            Reasons::LATEST,
        ];
        let rows = values
            .iter()
            .zip(reasons)
            .map(|((graph_id, value), reasons)| CompactRow {
                graph_id: GraphID(*graph_id),
                values: Values::from([(1, *value)]),
                reasons,
            })
            .collect::<Vec<_>>();
        let input = CompactInput {
            series: &rows,
            frames: &frames(&values),
            after_gap: &BTreeSet::from([GraphID(5)]),
            barriers: &BTreeSet::from([GraphID(2), GraphID(5)]),
            threshold: 50.0,
        };

        assert_eq!(
            compact_series(&input),
            CompactPlan::default(),
            "the +890 across the hole belongs to no diff, so neither a \
             crossing at 5 nor an anchor at 2 may be invented for it"
        );

        // Without the gap the very same rows are a crossing and its anchor.
        let plan = compact_series(&CompactInput {
            after_gap: &BTreeSet::new(),
            barriers: &BTreeSet::new(),
            ..input
        });
        k9::snapshot!(
            render(&apply(rows, &plan)),
            "
  1 10       FIRST
  2 10       ANCHOR
  5 900      OVER_THRESHOLD
  6 900      LATEST
"
        );
    }
}

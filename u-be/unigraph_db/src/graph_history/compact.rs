// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use unigraph_storage_core::GraphID;

use crate::graph_history::threshold::keep_row;

/// One of a node's stored rows in the range being compacted.
pub struct CompactRow {
    pub graph_id: GraphID,
    pub values: BTreeMap<u32, f64>,
    /// The row is already an anchor — see [`CompactPlan::anchored`].
    pub anchor: bool,
}

/// One node's stored series plus the context needed to judge its first row.
pub struct CompactInput<'a> {
    /// The node's rows inside the range being compacted, by ascending
    /// `graph_id`. Includes anchors.
    pub series: &'a [CompactRow],
    /// The node's last kept sample *before* the range, if any.
    ///
    /// Without it the first row in a bounded range compares against nothing and
    /// is always kept, so compacting `[a, b]` then `[b, c]` would keep more rows
    /// than compacting `[a, c]` in one pass. Seeding makes a windowed compaction
    /// produce exactly the same result as a whole-timeline one.
    pub seed: Option<&'a BTreeMap<u32, f64>>,
    /// Every *built* frame in the range, ascending.
    ///
    /// Which row anchors which sample is a frame question, not a row question:
    /// a sample's immediate predecessor usually has no row of its own, and the
    /// row before it in `series` can be thousands of frames back.
    pub frames: &'a [GraphID],
    pub threshold: f64,
}

/// What compaction should do to one node's rows in the range.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CompactPlan {
    /// Rows that are redundant at `threshold` and explain nothing. Delete them.
    pub dropped: Vec<GraphID>,
    /// Rows that are redundant on their own but sit at the built frame
    /// immediately before a surviving sample. They stay, flagged as anchors, so
    /// that sample's step reads as its own graph's contribution rather than as
    /// all the drift since the last kept row.
    ///
    /// Only rows that are not anchors *yet* appear here; ones that already are
    /// need no write.
    pub anchored: Vec<GraphID>,
}

/// Decide the fate of every row in one node's series.
///
/// Only ever call this on a range where every frame is settled — dropping a row
/// is as irreversible as never writing it, so a frame that fills later could
/// have made a dropped row significant. See [`crate::graph_history::settle`].
///
/// Anchors take no part in the threshold walk: one is below the threshold by
/// construction, so judging it would always drop it, and letting it advance the
/// baseline would hide the drift accumulated before it and swallow the very
/// sample it exists to explain. An anchor is therefore never promoted back to a
/// sample either — the walk simply does not see it.
///
/// A windowed pass cannot anchor across its lower bound: the predecessor of the
/// window's first survivor lies outside the range, so its row was never read.
/// Every row *inside* the window gets the same verdict a whole-series pass
/// would give it.
pub fn compact_series(input: &CompactInput<'_>) -> CompactPlan {
    let survivors = threshold_survivors(input);
    let wanted = anchor_frames(&survivors, input.frames);

    let mut plan = CompactPlan::default();
    for row in input.series {
        if survivors.contains(&row.graph_id) {
            continue;
        }
        if wanted.contains(&row.graph_id) {
            if !row.anchor {
                plan.anchored.push(row.graph_id);
            }
            continue;
        }
        plan.dropped.push(row.graph_id);
    }
    plan
}

/// The rows that clear the threshold, walking real samples only.
fn threshold_survivors(input: &CompactInput<'_>) -> BTreeSet<GraphID> {
    let mut last_kept = input.seed;
    let mut survivors = BTreeSet::new();

    for row in input.series.iter().filter(|row| !row.anchor) {
        if keep_row(last_kept, &row.values, input.threshold) {
            last_kept = Some(&row.values);
            survivors.insert(row.graph_id);
        }
    }
    survivors
}

/// The built frame immediately before each survivor, where the range holds one.
fn anchor_frames(survivors: &BTreeSet<GraphID>, frames: &[GraphID]) -> BTreeSet<GraphID> {
    survivors
        .iter()
        .filter_map(|graph_id| frames.binary_search(graph_id).ok())
        .filter_map(|index| index.checked_sub(1))
        .map(|index| frames[index])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(values: &[(i64, f64)]) -> Vec<CompactRow> {
        values
            .iter()
            .map(|(graph_id, value)| CompactRow {
                graph_id: GraphID(*graph_id),
                values: BTreeMap::from([(1, *value)]),
                anchor: false,
            })
            .collect()
    }

    /// Every graph ID in `series` is also a built frame, which is the usual
    /// shape for these fixtures — one row per frame before compaction.
    fn frames(values: &[(i64, f64)]) -> Vec<GraphID> {
        values
            .iter()
            .map(|(graph_id, _)| GraphID(*graph_id))
            .collect()
    }

    /// Apply a plan the way the storage layer does: drop, then flag.
    fn apply(rows: Vec<CompactRow>, plan: &CompactPlan) -> Vec<CompactRow> {
        rows.into_iter()
            .filter(|row| !plan.dropped.contains(&row.graph_id))
            .map(|row| CompactRow {
                anchor: row.anchor || plan.anchored.contains(&row.graph_id),
                ..row
            })
            .collect()
    }

    fn format_plan(plan: &CompactPlan) -> String {
        let ids = |list: &[GraphID]| {
            list.iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "dropped [{}]  anchored [{}]",
            ids(&plan.dropped),
            ids(&plan.anchored)
        )
    }

    #[test]
    fn compact_series_drops_until_drift_crosses_threshold() {
        let values = [
            (1, 0.0),
            (2, 4.0),
            (3, 9.0),
            (4, 10.0),
            (5, 14.0),
            (6, 20.0),
        ];
        let rows = series(&values);
        let frames = frames(&values);

        let plan = compact_series(&CompactInput {
            series: &rows,
            seed: None,
            frames: &frames,
            threshold: 10.0,
        });
        // Survivors are 1, 4 and 6. Rows 3 and 5 immediately precede 4 and 6,
        // so they stay as anchors; only 2 explains nothing and is dropped.
        k9::snapshot!(format_plan(&plan), "dropped [2]  anchored [3, 5]");

        let compacted = apply(rows, &plan);
        k9::snapshot!(
            format_plan(&compact_series(&CompactInput {
                series: &compacted,
                seed: None,
                frames: &frames,
                threshold: 10.0,
            })),
            "dropped []  anchored []"
        );
    }

    /// An anchor must never take part in the threshold walk. If it did, it
    /// would become the baseline for the sample it precedes — and since it sits
    /// within a threshold of that sample by construction, the sample would be
    /// dropped and the anchor kept. Exactly backwards.
    #[test]
    fn an_anchor_never_swallows_the_sample_it_explains() {
        let values = [(1, 0.0), (2, 95.0), (3, 100.0)];
        let mut rows = series(&values);
        rows[1].anchor = true;
        let frames = frames(&values);

        let plan = compact_series(&CompactInput {
            series: &rows,
            seed: None,
            frames: &frames,
            threshold: 10.0,
        });
        assert_eq!(
            plan,
            CompactPlan::default(),
            "graph 3 is +100 against its baseline at graph 1 and must survive, \
             and its anchor at graph 2 must stay to explain the +5 it contributed"
        );
    }

    /// An anchor whose sample no longer survives is just a wasted row.
    #[test]
    fn orphaned_anchors_are_reclaimed() {
        let values = [(1, 0.0), (2, 95.0), (3, 100.0)];
        let mut rows = series(&values);
        rows[1].anchor = true;
        let frames = frames(&values);

        // At this threshold graph 3 no longer clears the bar, so nothing needs
        // graph 2's row any more.
        let plan = compact_series(&CompactInput {
            series: &rows,
            seed: None,
            frames: &frames,
            threshold: 1000.0,
        });
        k9::snapshot!(format_plan(&plan), "dropped [2, 3]  anchored []");
    }

    /// A row only anchors a sample when it is that sample's immediate *frame*
    /// predecessor. The row before it in the series is not the same thing —
    /// most frames have no row at all.
    #[test]
    fn only_the_immediately_preceding_frame_anchors() {
        let values = [(1, 0.0), (5, 9.0), (9, 20.0)];
        let rows = series(&values);
        // Frames 1..9 all exist; the node just has no row at most of them.
        let frames = (1..=9).map(GraphID).collect::<Vec<_>>();

        let plan = compact_series(&CompactInput {
            series: &rows,
            seed: None,
            frames: &frames,
            threshold: 10.0,
        });
        assert_eq!(
            format_plan(&plan),
            "dropped [5]  anchored []",
            "graph 9's predecessor is frame 8, which has no row — graph 5 is not \
             an anchor for it and is simply redundant"
        );
    }

    /// A windowed compaction must agree with a whole-series one. Without the
    /// seed the window's first row is kept unconditionally, so `[4, 6]` alone
    /// would retain graph 4 even though it is redundant against graph 1.
    #[test]
    fn seed_makes_a_windowed_pass_match_the_whole_series() {
        let values = [
            (1, 0.0),
            (2, 4.0),
            (3, 9.0),
            (4, 10.0),
            (5, 14.0),
            (6, 20.0),
        ];
        let window_values = [(4, 10.0), (5, 14.0), (6, 20.0)];
        let whole = series(&values);
        let window = series(&window_values);
        let before_window = BTreeMap::from([(1, 0.0)]);

        let whole_plan = compact_series(&CompactInput {
            series: &whole,
            seed: None,
            frames: &frames(&values),
            threshold: 10.0,
        });
        let seeded = compact_series(&CompactInput {
            series: &window,
            seed: Some(&before_window),
            frames: &frames(&window_values),
            threshold: 10.0,
        });
        let unseeded = compact_series(&CompactInput {
            series: &window,
            seed: None,
            frames: &frames(&window_values),
            threshold: 10.0,
        });

        let in_window = |list: &[GraphID]| {
            list.iter()
                .copied()
                .filter(|id| id.0 >= 4)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            (seeded.dropped, seeded.anchored),
            (
                in_window(&whole_plan.dropped),
                in_window(&whole_plan.anchored)
            ),
            "a seeded window should reach the same verdict the whole-series pass \
             reaches for every row inside the window"
        );
        assert_eq!(
            format_plan(&unseeded),
            "dropped []  anchored [5]",
            "without a seed the window's first row survives even though it is redundant"
        );
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use unigraph_storage_core::GraphID;

use crate::graph_history::threshold::keep_row;

/// One node's stored series plus the context needed to judge its first row.
pub struct CompactInput<'a> {
    /// The node's kept rows inside the range being compacted, by ascending
    /// `graph_id`.
    pub series: &'a [(GraphID, BTreeMap<u32, f64>)],
    /// The node's last kept sample *before* the range, if any.
    ///
    /// Without it the first row in a bounded range compares against nothing and
    /// is always kept, so compacting `[a, b]` then `[b, c]` would keep more rows
    /// than compacting `[a, c]` in one pass. Seeding makes a windowed compaction
    /// produce exactly the same result as a whole-timeline one.
    pub seed: Option<&'a BTreeMap<u32, f64>>,
    pub threshold: f64,
}

/// Graph IDs whose rows are redundant at `threshold` and can be deleted.
///
/// Only ever call this on a range where every frame is settled — dropping a row
/// is as irreversible as never writing it, so a frame that fills later could
/// have made a dropped row significant. See [`crate::graph_history::settle`].
pub fn compact_series(input: &CompactInput<'_>) -> Vec<GraphID> {
    let mut last_kept = input.seed;
    let mut dropped = Vec::new();

    for (graph_id, values) in input.series {
        if keep_row(last_kept, values, input.threshold) {
            last_kept = Some(values);
        } else {
            dropped.push(*graph_id);
        }
    }

    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(values: &[(i64, f64)]) -> Vec<(GraphID, BTreeMap<u32, f64>)> {
        values
            .iter()
            .map(|(graph_id, value)| (GraphID(*graph_id), BTreeMap::from([(1, *value)])))
            .collect()
    }

    #[test]
    fn compact_series_drops_until_drift_crosses_threshold() {
        let series = series(&[
            (1, 0.0),
            (2, 4.0),
            (3, 9.0),
            (4, 10.0),
            (5, 14.0),
            (6, 20.0),
        ]);

        let dropped = compact_series(&CompactInput {
            series: &series,
            seed: None,
            threshold: 10.0,
        });
        k9::snapshot!(
            format!("{dropped:?}"),
            "[GraphID(2), GraphID(3), GraphID(5)]"
        );

        let compacted = series
            .into_iter()
            .filter(|(graph_id, _values)| !dropped.contains(graph_id))
            .collect::<Vec<_>>();
        assert!(
            compact_series(&CompactInput {
                series: &compacted,
                seed: None,
                threshold: 10.0,
            })
            .is_empty(),
            "compaction should be idempotent"
        );
    }

    /// A windowed compaction must agree with a whole-series one. Without the
    /// seed the window's first row is kept unconditionally, so `[4, 6]` alone
    /// would retain graph 4 even though it is redundant against graph 1.
    #[test]
    fn seed_makes_a_windowed_pass_match_the_whole_series() {
        let whole = series(&[
            (1, 0.0),
            (2, 4.0),
            (3, 9.0),
            (4, 10.0),
            (5, 14.0),
            (6, 20.0),
        ]);
        let window = series(&[(4, 10.0), (5, 14.0), (6, 20.0)]);
        let before_window = BTreeMap::from([(1, 0.0)]);

        let whole_dropped = compact_series(&CompactInput {
            series: &whole,
            seed: None,
            threshold: 10.0,
        });
        let seeded = compact_series(&CompactInput {
            series: &window,
            seed: Some(&before_window),
            threshold: 10.0,
        });
        let unseeded = compact_series(&CompactInput {
            series: &window,
            seed: None,
            threshold: 10.0,
        });

        assert_eq!(
            seeded,
            whole_dropped
                .iter()
                .copied()
                .filter(|id| id.0 >= 4)
                .collect::<Vec<_>>(),
            "a seeded window should drop exactly what the whole-series pass drops in that window"
        );
        assert_eq!(
            unseeded,
            vec![GraphID(5)],
            "without a seed the window's first row survives even though it is redundant"
        );
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Did this node move enough at this frame to be worth a row?
//!
//! # The one semantic everything else follows from
//!
//! ```text
//! crossing at N  <=>  |value(N) - value(N-1)| >= threshold      N-1 = previous BUILT frame
//! ```
//!
//! The comparison is against the **immediately preceding built frame**, not
//! against the node's last kept row. That is a product decision, not an
//! implementation detail: the series answers *"which diff moved this?"*, and a
//! diff that moved a bucket by less than the threshold did not move it enough
//! to name. Slow creep — plus one every frame, forever — is therefore
//! deliberately **not** recorded, because no single diff is to blame for it.
//! [`crate::graph_history::Reasons::LATEST`] is what keeps the
//! resulting error bounded.
//!
//! Everything hard about the old design came from the other choice. Measuring
//! against the last *kept* row makes a verdict depend on rows that may not
//! exist yet: a frame filling later, between the sample and its baseline,
//! invalidates a decision already taken — and an omission is permanent. So the
//! old design had to defer verdicts it could not trust, propagate that
//! provisionality forward, and track when a frame had stopped changing.
//!
//! Measuring against the immediately adjacent frame removes the whole category.
//! A frame cannot appear *between* two adjacent frames, so **no verdict is ever
//! provisional** and none of that machinery has anything to do.

use std::collections::BTreeMap;

/// One node's metric values at one frame, keyed by interned metric id.
pub type Values = BTreeMap<u32, f64>;

/// Did any metric move by at least `threshold` between the two frames?
///
/// `None` means the node was absent from that frame, which reads as zero for
/// every metric — the same as a metric that disappeared from a node that
/// stayed. A node dropping out of the graph is a real event worth a row, and
/// treating absence as "no change" would silently swallow it.
///
/// A single merge walk rather than two lookups per metric: both sides are
/// already sorted by metric id, and this runs once per node per frame.
pub fn crosses(previous: Option<&Values>, current: Option<&Values>, threshold: f64) -> bool {
    // An empty `BTreeMap` does not allocate, so standing one in for an absent
    // node costs nothing and keeps the walk below to a single shape.
    let absent = Values::new();

    let mut previous = previous.unwrap_or(&absent).iter().peekable();
    let mut current = current.unwrap_or(&absent).iter().peekable();

    loop {
        let (before, after) = match (previous.peek(), current.peek()) {
            (Some((left, before)), Some((right, after))) if left == right => {
                let step = (**before, **after);
                previous.next();
                current.next();
                step
            }
            // The lower id is missing from the other side, so it is zero there.
            (Some((left, before)), Some((right, _))) if left < right => {
                let step = (**before, 0.0);
                previous.next();
                step
            }
            (Some(_), Some((_, after))) => {
                let step = (0.0, **after);
                current.next();
                step
            }
            (Some((_, before)), None) => {
                let step = (**before, 0.0);
                previous.next();
                step
            }
            (None, Some((_, after))) => {
                let step = (0.0, **after);
                current.next();
                step
            }
            (None, None) => return false,
        };

        if (after - before).abs() >= threshold {
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(u32, f64)]) -> Values {
        pairs.iter().copied().collect()
    }

    #[test]
    fn a_crossing_is_an_absolute_move_in_any_single_metric() {
        let cases: [(Option<Values>, Option<Values>, f64, bool, &str); 10] = [
            (
                None,
                Some(values(&[(1, 10.0)])),
                100.0,
                false,
                "a node appearing below the threshold is not a crossing — \
                 Reasons::FIRST is what keeps its first row, not this",
            ),
            (
                None,
                Some(values(&[(1, 500.0)])),
                100.0,
                true,
                "a node appearing with real weight is a crossing",
            ),
            (
                Some(values(&[(1, 500.0)])),
                None,
                100.0,
                true,
                "a node dropping out of the graph is a crossing, not silence",
            ),
            (
                Some(values(&[(1, 10.0)])),
                Some(values(&[(1, 19.0)])),
                10.0,
                false,
                "below the threshold",
            ),
            (
                Some(values(&[(1, 10.0)])),
                Some(values(&[(1, 20.0)])),
                10.0,
                true,
                "exactly at the threshold counts — the comparison is >=",
            ),
            (
                Some(values(&[(1, 20.0)])),
                Some(values(&[(1, 9.0)])),
                10.0,
                true,
                "a drop is measured on its absolute size",
            ),
            (
                Some(values(&[(1, 1.0), (2, 1.0), (3, 1.0)])),
                Some(values(&[(1, 1.0), (2, 1.0), (3, 50.0)])),
                10.0,
                true,
                "one metric crossing is enough, whatever the others did",
            ),
            (
                Some(values(&[(1, 1.0)])),
                Some(values(&[(1, 1.0), (2, 50.0)])),
                10.0,
                true,
                "a metric appearing is a move from zero",
            ),
            (
                Some(values(&[(1, 50.0), (2, 2.0)])),
                Some(values(&[(2, 2.0)])),
                10.0,
                true,
                "a metric disappearing is a move to zero",
            ),
            (
                Some(values(&[(1, 5.0), (3, 5.0)])),
                Some(values(&[(2, 5.0), (4, 5.0)])),
                10.0,
                false,
                "interleaved ids each move by 5, so the merge walk must \
                 compare id-for-id rather than position-for-position",
            ),
        ];

        for (previous, current, threshold, expected, why) in cases {
            assert_eq!(
                crosses(previous.as_ref(), current.as_ref(), threshold),
                expected,
                "{why}"
            );
        }
    }

    /// The creep the new semantics deliberately does not record: every step is
    /// under the threshold, so no single diff is to blame and no row is kept —
    /// even though the total movement is many times the threshold.
    #[test]
    fn slow_creep_never_crosses_however_far_it_goes() {
        let series: Vec<Values> = (0..100).map(|step| values(&[(1, step as f64)])).collect();

        assert!(
            series
                .windows(2)
                .all(|pair| !crosses(Some(&pair[0]), Some(&pair[1]), 3.0)),
            "+1 per frame never crosses a threshold of 3"
        );
        assert!(
            crosses(Some(&series[0]), Some(&series[99]), 3.0),
            "the same movement measured end to end obviously does — which is \
             exactly the trade this semantic makes"
        );
    }
}

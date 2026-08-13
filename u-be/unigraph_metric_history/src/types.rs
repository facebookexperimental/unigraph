// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Core types for metric history.
//!
//! These types form the interface between the graph storage layer
//! (`unigraph_db`) and the history data structure (`FlatHistory`).

use std::collections::BTreeMap;
use std::fmt;

use anyhow::Result;
use unigraph_storage_core::GraphID;
use unigraph_timestamp::Timestamp;

/// Metric values for a single node at a single point in time.
///
/// Uses f64 for full precision (even though graph metrics are f64, we
/// promote to f64 during extraction to future-proof for f64 metrics).
/// Zero-valued metrics are excluded during extraction — see
/// [`crate::extract::extract_node_metrics`].
pub type NodeMetricSnapshot = BTreeMap<String, f64>;

/// A point in a timeline: `(Timestamp, GraphID)`.
///
/// Ordered lexicographically — first by timestamp, then by graph_id.
/// This matches the SQL `ORDER BY timestamp, graph_id` convention used
/// throughout the storage layer.
pub type Frame = (Timestamp, GraphID);

/// Per-node history entry: the metric snapshot at a specific frame.
///
/// - `Some(snapshot)` = node is present in the graph with these metrics
/// - `None` = node is absent from the graph at this frame
///
/// The `None` variant is critical for absent node tracking. Without it,
/// the sparse delta chain would incorrectly imply the node's previous
/// metrics persisted through frames where the node doesn't exist.
pub type MetricHistoryEntry = (Timestamp, Option<NodeMetricSnapshot>);

/// Map from `GraphID` to history entry. This is the input format for
/// [`FlatHistory::insert`](crate::FlatHistory::insert).
///
/// Keyed by `GraphID` rather than `Frame` so that duplicate entries for
/// the same graph (e.g., from re-ingestion) overwrite rather than create
/// parallel entries.
pub type MetricHistoryEntries = BTreeMap<GraphID, MetricHistoryEntry>;

/// ISO week partition key for grouping history blobs.
///
/// Uses ISO 8601 week numbering (1–53). The ISO week-year can differ
/// from the calendar year at year boundaries (e.g. Dec 31 can be W01
/// of the next year).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct WeekPartition {
    pub year: i32,
    pub week: u32,
}

impl WeekPartition {
    pub fn from_timestamp(ts: Timestamp) -> Self {
        Self {
            year: ts.iso_week_year(),
            week: ts.iso_week(),
        }
    }

    /// Format as `"YYYY-Www"` (e.g. `"2025-W03"`).
    pub fn display_key(&self) -> String {
        format!("{:04}-W{:02}", self.year, self.week)
    }

    /// Parse from `"YYYY-Www"` format.
    pub fn parse(s: &str) -> Result<Self> {
        let (year_str, week_str) = s.split_once("-W").ok_or_else(|| {
            anyhow::anyhow!("invalid week partition key: expected 'YYYY-Www', got '{s}'")
        })?;

        let year: i32 = year_str
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid year in week partition key: '{year_str}'"))?;

        let week: u32 = week_str
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid week in week partition key: '{week_str}'"))?;

        anyhow::ensure!(
            (1..=53).contains(&week),
            "week number must be 1–53, got {week}"
        );

        Ok(Self { year, week })
    }
}

impl fmt::Display for WeekPartition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-W{:02}", self.year, self.week)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_partition_display_and_parse() {
        let wp = WeekPartition {
            year: 2025,
            week: 3,
        };
        assert_eq!(wp.display_key(), "2025-W03");
        assert_eq!(wp.to_string(), "2025-W03");

        let parsed = WeekPartition::parse("2025-W03").unwrap();
        assert_eq!(parsed, wp);
    }

    #[test]
    fn week_partition_roundtrip() {
        for (year, week) in [(2024, 1), (2024, 52), (2020, 53), (2025, 27)] {
            let wp = WeekPartition { year, week };
            let key = wp.display_key();
            let parsed = WeekPartition::parse(&key).unwrap();
            assert_eq!(parsed, wp, "roundtrip failed for {key}");
        }
    }

    #[test]
    fn week_partition_from_timestamp() {
        // Mid-year
        let ts = Timestamp::from_rfc3339("2024-07-02T10:00:00Z").unwrap();
        let wp = WeekPartition::from_timestamp(ts);
        assert_eq!(wp.year, 2024);
        assert_eq!(wp.week, 27);

        // Year boundary: Dec 31, 2024 → ISO week 1 of 2025
        let ts = Timestamp::from_rfc3339("2024-12-31T00:00:00Z").unwrap();
        let wp = WeekPartition::from_timestamp(ts);
        assert_eq!(wp.year, 2025);
        assert_eq!(wp.week, 1);

        // W53: Dec 31, 2020 → ISO week 53 of 2020
        let ts = Timestamp::from_rfc3339("2020-12-31T00:00:00Z").unwrap();
        let wp = WeekPartition::from_timestamp(ts);
        assert_eq!(wp.year, 2020);
        assert_eq!(wp.week, 53);
    }

    #[test]
    fn week_partition_parse_errors() {
        assert!(WeekPartition::parse("2025-03").is_err()); // missing W
        assert!(WeekPartition::parse("2025-W00").is_err()); // week 0
        assert!(WeekPartition::parse("2025-W54").is_err()); // week 54
        assert!(WeekPartition::parse("abc-W03").is_err()); // bad year
        assert!(WeekPartition::parse("2025-Wxx").is_err()); // bad week
    }
}

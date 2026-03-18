// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Fundamental identifiers for timelines and graphs.
//!
//! These types are used across the entire Unigraph stack — from core graph
//! operations to storage to ingestion. They live in `unigraph_core` because
//! they are identity types, not storage-specific concepts.

use std::fmt;
use std::str::FromStr;

/// UTC timestamp — always stored and transmitted in UTC.
pub type Timestamp = unigraph_timestamp::Timestamp;

/// Unique identifier for a timeline — a named, ordered sequence of graphs.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub struct TimelineID(pub String);

impl fmt::Display for TimelineID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for TimelineID {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        anyhow::ensure!(!s.is_empty(), "TimelineID cannot be empty");
        Ok(TimelineID(s.to_string()))
    }
}

/// Unique identifier for a graph within a timeline.
///
/// Sequential integer assigned during ingestion. Sorts naturally for
/// correct frame ordering when multiple frames share the same timestamp.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub struct GraphID(pub i64);

/// Identifies a specific graph within a specific timeline.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct GraphKey {
    pub timeline_id: TimelineID,
    pub graph_id: GraphID,
}

impl fmt::Display for GraphKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}~{}", self.timeline_id.0, self.graph_id.0)
    }
}

impl FromStr for GraphKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        anyhow::ensure!(!s.is_empty(), "GraphKey cannot be empty");

        let (timeline, id) = s.rsplit_once('~').ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid GraphKey '{s}': expected format '<timeline>~<graph_id>' (e.g. 'cargo~356')"
            )
        })?;

        anyhow::ensure!(
            !timeline.is_empty(),
            "Invalid GraphKey '{s}': timeline_id is empty (before '~')"
        );

        anyhow::ensure!(
            !id.is_empty(),
            "Invalid GraphKey '{s}': graph_id is empty (after '~')"
        );

        let graph_id: i64 = id.parse().map_err(|_| {
            anyhow::anyhow!("Invalid GraphKey '{s}': graph_id '{id}' is not a valid integer")
        })?;

        Ok(GraphKey {
            timeline_id: TimelineID(timeline.to_string()),
            graph_id: GraphID(graph_id),
        })
    }
}

/// Identifies a specific graph within a timeline at a specific point in time.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct GraphTimeKey {
    pub timeline_id: TimelineID,
    pub timestamp: Timestamp,
    pub graph_id: GraphID,
}

impl GraphTimeKey {
    /// Extract the [`GraphKey`] (without timestamp) from this key.
    pub fn graph_key(&self) -> GraphKey {
        GraphKey {
            timeline_id: self.timeline_id.clone(),
            graph_id: self.graph_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_key_display() {
        let key = GraphKey {
            timeline_id: TimelineID("cargo".to_string()),
            graph_id: GraphID(356),
        };
        assert_eq!(key.to_string(), "cargo~356");
    }

    #[test]
    fn graph_key_parse() {
        let key: GraphKey = "cargo~356".parse().unwrap();
        assert_eq!(key.timeline_id, TimelineID("cargo".to_string()));
        assert_eq!(key.graph_id, GraphID(356));
    }

    #[test]
    fn graph_key_roundtrip() {
        let key = GraphKey {
            timeline_id: TimelineID("my_timeline".to_string()),
            graph_id: GraphID(42),
        };
        let parsed: GraphKey = key.to_string().parse().unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn graph_key_parse_errors() {
        // No separator
        let err = "noseparator".parse::<GraphKey>().unwrap_err();
        assert!(err.to_string().contains("expected format"), "{err}");

        // Empty input
        let err = "".parse::<GraphKey>().unwrap_err();
        assert!(err.to_string().contains("cannot be empty"), "{err}");

        // Empty timeline
        let err = "~123".parse::<GraphKey>().unwrap_err();
        assert!(err.to_string().contains("timeline_id is empty"), "{err}");

        // Empty graph_id
        let err = "cargo~".parse::<GraphKey>().unwrap_err();
        assert!(err.to_string().contains("graph_id is empty"), "{err}");

        // Non-integer graph_id
        let err = "cargo~abc".parse::<GraphKey>().unwrap_err();
        assert!(err.to_string().contains("not a valid integer"), "{err}");
    }

    #[test]
    fn timeline_id_display_and_parse() {
        let tid = TimelineID("cargo".to_string());
        assert_eq!(tid.to_string(), "cargo");

        let parsed: TimelineID = "cargo".parse().unwrap();
        assert_eq!(parsed, tid);
    }

    #[test]
    fn timeline_id_empty_fails() {
        let err = "".parse::<TimelineID>().unwrap_err();
        assert!(err.to_string().contains("cannot be empty"), "{err}");
    }
}

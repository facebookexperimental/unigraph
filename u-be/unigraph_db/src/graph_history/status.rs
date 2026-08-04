// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::fmt;
use std::str::FromStr;

use anyhow::Result;

use crate::graph_history::STATUS_EMPTY;
use crate::graph_history::STATUS_ERROR;
use crate::graph_history::STATUS_OMITTED;
use crate::graph_history::STATUS_PROCESSED;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HistoryStatus {
    Processed,
    Omitted,
    Error,
    Empty,
}

impl fmt::Display for HistoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryStatus::Processed => write!(f, "{STATUS_PROCESSED}"),
            HistoryStatus::Omitted => write!(f, "{STATUS_OMITTED}"),
            HistoryStatus::Error => write!(f, "{STATUS_ERROR}"),
            HistoryStatus::Empty => write!(f, "{STATUS_EMPTY}"),
        }
    }
}

impl FromStr for HistoryStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            STATUS_PROCESSED => Ok(HistoryStatus::Processed),
            STATUS_OMITTED => Ok(HistoryStatus::Omitted),
            STATUS_ERROR => Ok(HistoryStatus::Error),
            STATUS_EMPTY => Ok(HistoryStatus::Empty),
            other => Err(anyhow::anyhow!("Unknown HistoryStatus: {}", other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorPayload {
    pub messages: Vec<String>,
    pub details: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips_exact_variant_names() {
        for status in [
            HistoryStatus::Processed,
            HistoryStatus::Omitted,
            HistoryStatus::Error,
            HistoryStatus::Empty,
        ] {
            let encoded = status.to_string();
            assert_eq!(
                encoded.parse::<HistoryStatus>().expect("status parses"),
                status
            );
        }
    }
}

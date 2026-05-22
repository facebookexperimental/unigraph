// Copyright (c) Meta Platforms, Inc. and affiliates.

use unigraph_timestamp::Timestamp;

/// Metadata for a single git commit.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// Full 40-character hex SHA.
    pub hash: String,
    /// Commit timestamp (author date), converted to UTC.
    pub timestamp: Timestamp,
    /// First line of the commit message.
    pub summary: String,
}

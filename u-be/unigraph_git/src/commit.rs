// Copyright (c) Meta Platforms, Inc. and affiliates.

use chrono::DateTime;
use chrono::Utc;

/// Metadata for a single git commit.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// Full 40-character hex SHA.
    pub hash: String,
    /// Commit timestamp (author date), converted to UTC.
    pub timestamp: DateTime<Utc>,
    /// First line of the commit message.
    pub summary: String,
}

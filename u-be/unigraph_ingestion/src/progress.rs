// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::cmp;

/// Progress reporter that prints to stderr.
pub struct IngestionProgress {
    total: usize,
    current: usize,
}

impl IngestionProgress {
    pub fn new(total: usize) -> Self {
        Self { total, current: 0 }
    }

    /// Advance the counter without printing anything.
    pub fn skip_silent(&mut self) {
        self.current += 1;
    }

    pub fn start(&mut self, hash: &str, summary: &str) {
        self.current += 1;
        let short = &hash[..cmp::min(8, hash.len())];
        eprintln!(
            "[{}/{}] Processing {} - {}",
            self.current, self.total, short, summary
        );
    }

    pub fn error(&self, err: &anyhow::Error) {
        eprintln!("  ERROR: {err:#}");
    }

    pub fn done(&self, stored: usize, skipped: usize, errors: usize) {
        eprintln!(
            "Ingestion complete: {} stored, {} skipped, {} errors",
            stored, skipped, errors
        );
    }
}

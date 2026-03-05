// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SQLite backend for the Unigraph storage layer.
//!
//! [`SqliteStorage`] implements both [`UnigraphGraphStorage`](unigraph_storage_core::UnigraphGraphStorage)
//! and [`UnigraphBlobStorage`](unigraph_storage_core::UnigraphBlobStorage) using a single
//! SQLite database (via `rusqlite` with bundled SQLite).

mod blob;
mod graph;
mod schema;

use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;

/// SQLite-backed storage for Unigraph timelines, frames, and blobs.
///
/// Thread-safe via an internal `Mutex<Connection>`.
///
/// # Examples
///
/// ```rust
/// use unigraph_storage_sqlite::SqliteStorage;
///
/// // In-memory database (for testing)
/// let storage = SqliteStorage::new_in_memory().unwrap();
///
/// // File-based database
/// // let storage = SqliteStorage::new("path/to/db.sqlite").unwrap();
/// ```
pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    /// Create a new SQLite storage backed by a file on disk.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path).context("Failed to open SQLite database")?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Create a new in-memory SQLite storage (useful for testing).
    pub fn new_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().context("Failed to open in-memory SQLite database")?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Run the DDL statements to create tables and indices.
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(schema::CREATE_SCHEMA)
            .context("Failed to initialize SQLite schema")?;
        Ok(())
    }
}

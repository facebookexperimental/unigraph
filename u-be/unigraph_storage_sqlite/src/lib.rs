// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SQLite backend for the Unigraph storage layer.
//!
//! [`SqliteStorage`] implements [`UnigraphGraphStorage`](unigraph_storage_core::UnigraphGraphStorage)
//! (via [`SqliteConnection`]) and [`UnigraphBlobStorage`](unigraph_storage_core::UnigraphBlobStorage)
//! using a single SQLite database (via `rusqlite` with bundled SQLite).

mod blob;
mod graph;
mod history;
mod schema;

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

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
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// Create a new SQLite storage backed by a file on disk.
    ///
    /// Creates parent directories if they don't exist.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create parent directories for {}",
                    path.as_ref().display()
                )
            })?;
        }
        let conn = Connection::open(path).context("Failed to open SQLite database")?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Create a new in-memory SQLite storage (useful for testing).
    pub fn new_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().context("Failed to open in-memory SQLite database")?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
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

/// A connection to a SQLite-backed graph storage.
///
/// Holds a reference to the shared `Arc<Mutex<Connection>>` and acquires
/// the lock for each operation. Tracks transaction state via an `AtomicBool`
/// so the connection is `Send + Sync`.
///
/// If a transaction is active when the connection is dropped, it is
/// automatically rolled back.
pub struct SqliteConnection {
    conn: Arc<Mutex<Connection>>,
    transaction_active: bool,
}

impl SqliteConnection {
    /// Acquire the mutex lock on the underlying connection.
    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

impl Drop for SqliteConnection {
    fn drop(&mut self) {
        if self.transaction_active {
            let conn = self.conn.lock().unwrap();
            let _ = conn.execute("ROLLBACK", []);
        }
    }
}

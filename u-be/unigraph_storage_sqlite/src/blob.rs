// Copyright (c) Meta Platforms, Inc. and affiliates.

//! [`UnigraphBlobStorage`] implementation for SQLite.

use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use unigraph_storage_core::traits::UnigraphBlobStorage;

use crate::SqliteStorage;

impl UnigraphBlobStorage for SqliteStorage {
    fn put_blob(&self, key: &str, data: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO blobs (blob_key, data, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, data, now],
        )
        .with_context(|| format!("Failed to put blob: {}", key))?;

        Ok(())
    }

    fn get_blob(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT data FROM blobs WHERE blob_key = ?1")
            .context("Failed to prepare get_blob query")?;

        let result = stmt
            .query_row(rusqlite::params![key], |row| row.get::<_, Vec<u8>>(0))
            .optional()
            .with_context(|| format!("Failed to get blob: {}", key))?;

        Ok(result)
    }

    fn delete_blob(&self, key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM blobs WHERE blob_key = ?1",
            rusqlite::params![key],
        )
        .with_context(|| format!("Failed to delete blob: {}", key))?;

        Ok(())
    }

    fn has_blob(&self, key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT 1 FROM blobs WHERE blob_key = ?1")
            .context("Failed to prepare has_blob query")?;

        let exists = stmt
            .query_row(rusqlite::params![key], |_| Ok(()))
            .optional()
            .with_context(|| format!("Failed to check blob: {}", key))?
            .is_some();

        Ok(exists)
    }

    fn list_blobs(&self, prefix: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("{}%", prefix);
        let mut stmt = conn
            .prepare("SELECT blob_key FROM blobs WHERE blob_key LIKE ?1 ORDER BY blob_key")
            .context("Failed to prepare list_blobs query")?;

        let rows = stmt
            .query_map(rusqlite::params![pattern], |row| row.get::<_, String>(0))
            .context("Failed to query blobs")?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read blob key")?);
        }
        Ok(result)
    }
}

/// Extension trait to add `.optional()` to rusqlite results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

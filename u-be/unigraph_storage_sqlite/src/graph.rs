// Copyright (c) Meta Platforms, Inc. and affiliates.

//! [`UnigraphGraphStorage`] implementation for SQLite.

use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use unigraph_storage_core::FrameData;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::frame::Frame;
use unigraph_storage_core::traits::UnigraphGraphStorage;
use unigraph_storage_core::types::GraphID;

use crate::SqliteStorage;

impl UnigraphGraphStorage for SqliteStorage {
    fn create_timeline(&self, timeline_id: &TimelineID, config: &TimelineConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let config_json =
            serde_json::to_string(config).context("Failed to serialize TimelineConfig")?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO timelines (timeline_id, config_json, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![timeline_id.0, config_json, now],
        )
        .context("Failed to insert timeline")?;

        Ok(())
    }

    fn get_timeline_config(&self, timeline_id: &TimelineID) -> Result<Option<TimelineConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT config_json FROM timelines WHERE timeline_id = ?1")
            .context("Failed to prepare timeline query")?;

        let result = stmt
            .query_row(rusqlite::params![timeline_id.0], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .context("Failed to query timeline config")?;

        match result {
            Some(json) => {
                let config: TimelineConfig =
                    serde_json::from_str(&json).context("Failed to parse TimelineConfig")?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    fn list_timelines(&self) -> Result<Vec<TimelineID>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT timeline_id FROM timelines ORDER BY timeline_id")
            .context("Failed to prepare list timelines query")?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("Failed to query timelines")?;

        let mut result = Vec::new();
        for row in rows {
            result.push(TimelineID(row.context("Failed to read timeline row")?));
        }
        Ok(result)
    }

    fn store_frame(
        &self,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let timestamp = key.timestamp.to_rfc3339();
        let frame_type_str = frame_type.to_string();
        let base_key_json = base
            .map(serde_json::to_string)
            .transpose()
            .context("Failed to serialize base GraphKey")?;

        conn.execute(
            "INSERT INTO frames (timeline_id, timestamp, graph_id, frame_type, manifest_json, inline_blobs, base_key_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                key.timeline_id.0,
                timestamp,
                key.graph_id.0,
                frame_type_str,
                manifest_json,
                inline_blobs,
                base_key_json,
                now,
            ],
        )
        .context("Failed to insert frame")?;

        Ok(())
    }

    fn store_frame_empty(&self, key: &GraphTimeKey) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let timestamp = key.timestamp.to_rfc3339();

        conn.execute(
            "INSERT INTO frames (timeline_id, timestamp, graph_id, frame_type, manifest_json, inline_blobs, base_key_json, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5)",
            rusqlite::params![
                key.timeline_id.0,
                timestamp,
                key.graph_id.0,
                "Empty",
                now,
            ],
        )
        .context("Failed to insert empty frame")?;

        Ok(())
    }

    fn get_frame(&self, key: &GraphKey, with_data: bool) -> Result<Option<FrameRow>> {
        let conn = self.conn.lock().unwrap();

        if with_data {
            let mut stmt = conn
                .prepare(
                    "SELECT timestamp, frame_type, base_key_json, manifest_json, inline_blobs
                     FROM frames
                     WHERE timeline_id = ?1 AND graph_id = ?2",
                )
                .context("Failed to prepare get_frame query")?;

            stmt.query_row(
                rusqlite::params![key.timeline_id.0, key.graph_id.0],
                |row| {
                    let timestamp_str: String = row.get(0)?;
                    let frame_type_str: String = row.get(1)?;
                    let base_key_json: Option<String> = row.get(2)?;
                    let manifest_json: Option<String> = row.get(3)?;
                    let inline_blobs: Option<Vec<u8>> = row.get(4)?;
                    Ok((
                        timestamp_str,
                        frame_type_str,
                        base_key_json,
                        manifest_json,
                        inline_blobs,
                    ))
                },
            )
            .optional()
            .context("Failed to query frame")?
            .map(
                |(timestamp_str, frame_type_str, base_key_json, manifest_json, inline_blobs)| {
                    let timestamp = parse_timestamp(&timestamp_str)?;
                    let frame_type: FrameType = frame_type_str
                        .parse()
                        .context("Failed to parse FrameType")?;
                    let base = parse_base_key(base_key_json.as_deref())?;
                    let data = manifest_json.map(|mj| FrameData {
                        manifest_json: mj,
                        inline_blobs,
                    });

                    Ok(FrameRow {
                        frame: Frame {
                            timestamp,
                            graph_id: key.graph_id.clone(),
                        },
                        timeline_id: key.timeline_id.clone(),
                        frame_type,
                        base,
                        data,
                    })
                },
            )
            .transpose()
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT timestamp, frame_type, base_key_json
                     FROM frames
                     WHERE timeline_id = ?1 AND graph_id = ?2",
                )
                .context("Failed to prepare get_frame metadata query")?;

            stmt.query_row(
                rusqlite::params![key.timeline_id.0, key.graph_id.0],
                |row| {
                    let timestamp_str: String = row.get(0)?;
                    let frame_type_str: String = row.get(1)?;
                    let base_key_json: Option<String> = row.get(2)?;
                    Ok((timestamp_str, frame_type_str, base_key_json))
                },
            )
            .optional()
            .context("Failed to query frame metadata")?
            .map(|(timestamp_str, frame_type_str, base_key_json)| {
                let timestamp = parse_timestamp(&timestamp_str)?;
                let frame_type: FrameType = frame_type_str
                    .parse()
                    .context("Failed to parse FrameType")?;
                let base = parse_base_key(base_key_json.as_deref())?;

                Ok(FrameRow {
                    frame: Frame {
                        timestamp,
                        graph_id: key.graph_id.clone(),
                    },
                    timeline_id: key.timeline_id.clone(),
                    frame_type,
                    base,
                    data: None,
                })
            })
            .transpose()
        }
    }

    fn list_frames(&self, timeline_id: &TimelineID) -> Result<Vec<FrameRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT graph_id, timestamp, frame_type, base_key_json
                 FROM frames
                 WHERE timeline_id = ?1
                 ORDER BY timestamp, graph_id",
            )
            .context("Failed to prepare list_frames query")?;

        let rows = stmt
            .query_map(rusqlite::params![timeline_id.0], |row| {
                let graph_id: String = row.get(0)?;
                let timestamp_str: String = row.get(1)?;
                let frame_type_str: String = row.get(2)?;
                let base_key_json: Option<String> = row.get(3)?;
                Ok((graph_id, timestamp_str, frame_type_str, base_key_json))
            })
            .context("Failed to query frames")?;

        let mut result = Vec::new();
        for row in rows {
            let (graph_id, timestamp_str, frame_type_str, base_key_json) =
                row.context("Failed to read frame row")?;
            let timestamp = parse_timestamp(&timestamp_str)?;
            let frame_type: FrameType = frame_type_str
                .parse()
                .context("Failed to parse FrameType")?;
            let base = parse_base_key(base_key_json.as_deref())?;

            result.push(FrameRow {
                frame: Frame {
                    timestamp,
                    graph_id: GraphID(graph_id),
                },
                timeline_id: timeline_id.clone(),
                frame_type,
                base,
                data: None,
            });
        }
        Ok(result)
    }

    fn list_frames_range(
        &self,
        timeline_id: &TimelineID,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Vec<FrameRow>> {
        let conn = self.conn.lock().unwrap();
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        let mut stmt = conn
            .prepare(
                "SELECT graph_id, timestamp, frame_type, base_key_json
                 FROM frames
                 WHERE timeline_id = ?1 AND timestamp >= ?2 AND timestamp <= ?3
                 ORDER BY timestamp, graph_id",
            )
            .context("Failed to prepare list_frames_range query")?;

        let rows = stmt
            .query_map(
                rusqlite::params![timeline_id.0, start_str, end_str],
                |row| {
                    let graph_id: String = row.get(0)?;
                    let timestamp_str: String = row.get(1)?;
                    let frame_type_str: String = row.get(2)?;
                    let base_key_json: Option<String> = row.get(3)?;
                    Ok((graph_id, timestamp_str, frame_type_str, base_key_json))
                },
            )
            .context("Failed to query frames range")?;

        let mut result = Vec::new();
        for row in rows {
            let (graph_id, timestamp_str, frame_type_str, base_key_json) =
                row.context("Failed to read frame row")?;
            let timestamp = parse_timestamp(&timestamp_str)?;
            let frame_type: FrameType = frame_type_str
                .parse()
                .context("Failed to parse FrameType")?;
            let base = parse_base_key(base_key_json.as_deref())?;

            result.push(FrameRow {
                frame: Frame {
                    timestamp,
                    graph_id: GraphID(graph_id),
                },
                timeline_id: timeline_id.clone(),
                frame_type,
                base,
                data: None,
            });
        }
        Ok(result)
    }

    fn get_preceding_frame(&self, key: &GraphTimeKey) -> Result<Option<FrameRow>> {
        let conn = self.conn.lock().unwrap();
        let timestamp_str = key.timestamp.to_rfc3339();

        let mut stmt = conn
            .prepare(
                "SELECT graph_id, timestamp, frame_type, base_key_json
                 FROM frames
                 WHERE timeline_id = ?1
                   AND (timestamp < ?2 OR (timestamp = ?2 AND graph_id < ?3))
                 ORDER BY timestamp DESC, graph_id DESC
                 LIMIT 1",
            )
            .context("Failed to prepare get_preceding_frame query")?;

        stmt.query_row(
            rusqlite::params![key.timeline_id.0, timestamp_str, key.graph_id.0],
            |row| {
                let graph_id: String = row.get(0)?;
                let ts_str: String = row.get(1)?;
                let frame_type_str: String = row.get(2)?;
                let base_key_json: Option<String> = row.get(3)?;
                Ok((graph_id, ts_str, frame_type_str, base_key_json))
            },
        )
        .optional()
        .context("Failed to query preceding frame")?
        .map(|(graph_id, ts_str, frame_type_str, base_key_json)| {
            let timestamp = parse_timestamp(&ts_str)?;
            let frame_type: FrameType = frame_type_str
                .parse()
                .context("Failed to parse FrameType")?;
            let base = parse_base_key(base_key_json.as_deref())?;

            Ok(FrameRow {
                frame: Frame {
                    timestamp,
                    graph_id: GraphID(graph_id),
                },
                timeline_id: key.timeline_id.clone(),
                frame_type,
                base,
                data: None,
            })
        })
        .transpose()
    }

    fn register_blobs_for_cleanup(&self, blob_keys: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let mut stmt = conn
            .prepare("INSERT OR IGNORE INTO blobs_to_delete (blob_key, created_at) VALUES (?1, ?2)")
            .context("Failed to prepare register_blobs_for_cleanup")?;

        for key in blob_keys {
            stmt.execute(rusqlite::params![key, now])
                .context("Failed to register blob for cleanup")?;
        }
        Ok(())
    }

    fn unregister_blobs_for_cleanup(&self, blob_keys: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("DELETE FROM blobs_to_delete WHERE blob_key = ?1")
            .context("Failed to prepare unregister_blobs_for_cleanup")?;

        for key in blob_keys {
            stmt.execute(rusqlite::params![key])
                .context("Failed to unregister blob from cleanup")?;
        }
        Ok(())
    }

    fn get_blobs_pending_cleanup(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT blob_key FROM blobs_to_delete ORDER BY blob_key")
            .context("Failed to prepare get_blobs_pending_cleanup")?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("Failed to query blobs pending cleanup")?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read blob key")?);
        }
        Ok(result)
    }
}

/// Parse a timestamp string (RFC 3339 / ISO 8601) into a `Timestamp`.
fn parse_timestamp(s: &str) -> Result<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .with_context(|| format!("Failed to parse timestamp: {}", s))
}

/// Parse an optional base key JSON string into a `GraphKey`.
fn parse_base_key(json: Option<&str>) -> Result<Option<GraphKey>> {
    json.map(|s| serde_json::from_str(s).context("Failed to parse base GraphKey"))
        .transpose()
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

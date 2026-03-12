// Copyright (c) Meta Platforms, Inc. and affiliates.

//! [`UnigraphGraphStorage`] and [`UnigraphGraphConnection`] implementation for SQLite.

use std::sync::MutexGuard;

use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use rusqlite::Connection;
use unigraph_storage_core::ExternalID;
use unigraph_storage_core::ExternalIDNamespace;
use unigraph_storage_core::FrameData;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::frame::Frame;
use unigraph_storage_core::traits::UnigraphGraphConnection;
use unigraph_storage_core::traits::UnigraphGraphStorage;
use unigraph_timestamp::Timestamp;

use crate::SqliteConnection;
use crate::SqliteStorage;
use crate::schema::TABLE_BLOBS_TO_DELETE;
use crate::schema::TABLE_EXTERNAL_ID_MAPPINGS;
use crate::schema::TABLE_GRAPHS;
use crate::schema::TABLE_METRIC_HISTORY;
use crate::schema::TABLE_TIMELINE_CONFIGS;

#[async_trait]
impl UnigraphGraphStorage for SqliteStorage {
    async fn conn(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        Ok(Box::new(SqliteConnection {
            conn: self.conn.clone(),
            transaction_active: false,
        }))
    }
}

#[async_trait]
impl UnigraphGraphConnection for SqliteConnection {
    async fn start_transaction(&mut self) -> Result<()> {
        self.lock()
            .execute("BEGIN EXCLUSIVE", [])
            .context("Failed to begin exclusive transaction")?;
        self.transaction_active = true;
        Ok(())
    }

    async fn commit_transaction(&mut self) -> Result<()> {
        self.lock()
            .execute("COMMIT", [])
            .context("Failed to commit transaction")?;
        self.transaction_active = false;
        Ok(())
    }

    async fn create_timeline(
        &mut self,
        timeline_id: &TimelineID,
        config: &TimelineConfig,
    ) -> Result<()> {
        let config_json =
            serde_json::to_string(config).context("Failed to serialize TimelineConfig")?;
        let now = Timestamp::now().to_unix_timestamp();

        let conn = self.lock();
        let sql = format!(
            "INSERT INTO {} (timeline_id, config_json, created_at) VALUES (?1, ?2, ?3)",
            TABLE_TIMELINE_CONFIGS
        );
        conn.execute(&sql, rusqlite::params![timeline_id.0, config_json, now])
            .context("Failed to insert timeline")?;

        Ok(())
    }

    async fn get_timeline_config(
        &mut self,
        timeline_id: &TimelineID,
    ) -> Result<Option<TimelineConfig>> {
        let conn = self.lock();
        query_timeline_config(&conn, timeline_id)
    }

    async fn get_timeline_config_and_lock(
        &mut self,
        timeline_id: &TimelineID,
    ) -> Result<Option<TimelineConfig>> {
        // For SQLite, BEGIN EXCLUSIVE (in start_transaction) already serializes
        // all writers. The caller has already started the transaction, so we
        // just read the config here.
        let conn = self.lock();
        query_timeline_config(&conn, timeline_id)
    }

    async fn list_timelines(&mut self) -> Result<Vec<TimelineID>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT timeline_id FROM {} ORDER BY timeline_id",
            TABLE_TIMELINE_CONFIGS
        );
        let mut stmt = conn
            .prepare(&sql)
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

    async fn store_frame(
        &mut self,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
    ) -> Result<()> {
        let now = Timestamp::now().to_unix_timestamp();
        let timestamp = key.timestamp.to_unix_timestamp();
        let frame_type_str = frame_type.to_string();
        let base_key_json = base
            .map(serde_json::to_string)
            .transpose()
            .context("Failed to serialize base GraphKey")?;

        let conn = self.lock();

        // Delete an existing Empty frame so we can replace it, then insert.
        let delete_sql = format!(
            "DELETE FROM {} WHERE timeline_id = ?1 AND graph_id = ?2 AND frame_type = 'Empty'",
            TABLE_GRAPHS
        );
        conn.execute(
            &delete_sql,
            rusqlite::params![key.timeline_id.0, key.graph_id.0],
        )
        .context("Failed to delete existing empty frame")?;

        let insert_sql = format!(
            "INSERT INTO {} (timeline_id, timestamp, graph_id, frame_type, manifest_json, inline_blobs, base_key_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            TABLE_GRAPHS
        );
        conn.execute(
            &insert_sql,
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

    async fn store_frame_empty(&mut self, key: &GraphTimeKey) -> Result<()> {
        let now = Timestamp::now().to_unix_timestamp();
        let timestamp = key.timestamp.to_unix_timestamp();

        let conn = self.lock();
        let sql = format!(
            "INSERT INTO {} (timeline_id, timestamp, graph_id, frame_type, manifest_json, inline_blobs, base_key_json, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5)",
            TABLE_GRAPHS
        );
        conn.execute(
            &sql,
            rusqlite::params![key.timeline_id.0, timestamp, key.graph_id.0, "Empty", now,],
        )
        .context("Failed to insert empty frame")?;

        Ok(())
    }

    async fn select_frames(&mut self, query: &FrameQuery) -> Result<Vec<FrameRow>> {
        let with_data = query.with_data.unwrap_or(false);

        // Build SELECT columns.
        let select = if with_data {
            format!(
                "SELECT graph_id, timestamp, frame_type, base_key_json, manifest_json, inline_blobs FROM {}",
                TABLE_GRAPHS
            )
        } else {
            format!(
                "SELECT graph_id, timestamp, frame_type, base_key_json FROM {}",
                TABLE_GRAPHS
            )
        };

        // Build WHERE clauses and collect params as strings.
        let mut conditions: Vec<String> = vec!["timeline_id = ?1".to_string()];
        let mut params: Vec<String> = vec![query.timeline_id.0.clone()];

        if let Some(bounds) = &query.timestamp_bounds {
            if let Some(start) = &bounds.start {
                params.push(start.to_unix_timestamp().to_string());
                conditions.push(format!("timestamp >= ?{}", params.len()));
            }
            if let Some(end) = &bounds.end {
                params.push(end.to_unix_timestamp().to_string());
                conditions.push(format!("timestamp <= ?{}", params.len()));
            }
        }

        if let Some(bounds) = &query.graph_id_bounds {
            if let Some(lower) = &bounds.0 {
                params.push(lower.0.to_string());
                conditions.push(format!("graph_id >= ?{}", params.len()));
            }
            if let Some(upper) = &bounds.1 {
                params.push(upper.0.to_string());
                conditions.push(format!("graph_id <= ?{}", params.len()));
            }
        }

        if let Some(ids) = &query.graph_ids {
            let placeholders: Vec<String> = ids
                .iter()
                .map(|id| {
                    params.push(id.0.to_string());
                    format!("?{}", params.len())
                })
                .collect();
            conditions.push(format!("graph_id IN ({})", placeholders.join(", ")));
        }

        if let Some(types) = &query.frame_types {
            let placeholders: Vec<String> = types
                .iter()
                .map(|ft| {
                    params.push(ft.to_string());
                    format!("?{}", params.len())
                })
                .collect();
            conditions.push(format!("frame_type IN ({})", placeholders.join(", ")));
        }

        if let Some((ts, gid)) = &query.before {
            let ts_str = ts.to_unix_timestamp().to_string();
            params.push(ts_str.clone());
            let ts_idx = params.len();
            params.push(ts_str);
            let ts_idx2 = params.len();
            params.push(gid.0.to_string());
            let gid_idx = params.len();
            conditions.push(format!(
                "(timestamp < ?{} OR (timestamp = ?{} AND graph_id < ?{}))",
                ts_idx, ts_idx2, gid_idx
            ));
        }

        // Build ORDER BY.
        let order_clause = if query.before.is_some() {
            // `before` implies DESC to get the closest preceding frame.
            "ORDER BY timestamp DESC, graph_id DESC"
        } else {
            match query.order.as_ref().unwrap_or(&Order::Asc) {
                Order::Asc => "ORDER BY timestamp, graph_id",
                Order::Desc => "ORDER BY timestamp DESC, graph_id DESC",
            }
        };

        // Build LIMIT.
        let limit_clause = if query.before.is_some() {
            // `before` implies LIMIT 1.
            "LIMIT 1".to_string()
        } else if let Some(limit) = query.limit {
            format!("LIMIT {}", limit)
        } else {
            String::new()
        };

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "{} WHERE {} {} {}",
            select, where_clause, order_clause, limit_clause
        );

        let conn = self.lock();
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare select_frames query")?;

        // Convert params to rusqlite dynamic params.
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let mut result = Vec::new();
        let mut rows = stmt
            .query(param_refs.as_slice())
            .context("Failed to execute select_frames query")?;

        while let Some(row) = rows.next().context("Failed to read frame row")? {
            let graph_id: i64 = row.get(0)?;
            let timestamp_unix: i64 = row.get(1)?;
            let frame_type_str: String = row.get(2)?;
            let base_key_json: Option<String> = row.get(3)?;

            let timestamp = Timestamp::from_unix_timestamp(timestamp_unix);
            let frame_type: FrameType = frame_type_str
                .parse()
                .context("Failed to parse FrameType")?;
            let base = parse_base_key(base_key_json.as_deref())?;

            let data = if with_data {
                let manifest_json: Option<String> = row.get(4)?;
                let inline_blobs: Option<Vec<u8>> = row.get(5)?;
                manifest_json.map(|mj| FrameData {
                    manifest_json: mj,
                    inline_blobs,
                })
            } else {
                None
            };

            result.push(FrameRow {
                frame: Frame {
                    timestamp,
                    graph_id: GraphID(graph_id),
                },
                timeline_id: query.timeline_id.clone(),
                frame_type,
                base,
                data,
            });
        }

        Ok(result)
    }

    async fn delete_frame(&mut self, key: &GraphKey) -> Result<bool> {
        let conn = self.lock();
        let sql = format!(
            "DELETE FROM {} WHERE timeline_id = ?1 AND graph_id = ?2",
            TABLE_GRAPHS
        );
        let deleted = conn
            .execute(&sql, rusqlite::params![key.timeline_id.0, key.graph_id.0])
            .context("Failed to delete frame")?;
        Ok(deleted > 0)
    }

    async fn register_blobs_for_cleanup(&mut self, blob_keys: &[String]) -> Result<()> {
        let now = Timestamp::now().to_unix_timestamp();
        let conn = self.lock();
        let sql = format!(
            "INSERT OR IGNORE INTO {} (blob_key, created_at) VALUES (?1, ?2)",
            TABLE_BLOBS_TO_DELETE
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare register_blobs_for_cleanup")?;

        for key in blob_keys {
            stmt.execute(rusqlite::params![key, now])
                .context("Failed to register blob for cleanup")?;
        }
        Ok(())
    }

    async fn unregister_blobs_for_cleanup(&mut self, blob_keys: &[String]) -> Result<()> {
        let conn = self.lock();
        let sql = format!("DELETE FROM {} WHERE blob_key = ?1", TABLE_BLOBS_TO_DELETE);
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare unregister_blobs_for_cleanup")?;

        for key in blob_keys {
            stmt.execute(rusqlite::params![key])
                .context("Failed to unregister blob from cleanup")?;
        }
        Ok(())
    }

    async fn get_blobs_pending_cleanup(&mut self) -> Result<Vec<String>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT blob_key FROM {} ORDER BY blob_key",
            TABLE_BLOBS_TO_DELETE
        );
        let mut stmt = conn
            .prepare(&sql)
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

    async fn get_blobs_pending_cleanup_older_than(
        &mut self,
        older_than: unigraph_storage_core::Timestamp,
    ) -> Result<Vec<String>> {
        let cutoff = older_than.to_unix_timestamp();
        let conn = self.lock();
        let sql = format!(
            "SELECT blob_key FROM {}
             WHERE created_at <= ?1
             ORDER BY blob_key",
            TABLE_BLOBS_TO_DELETE
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare get_blobs_pending_cleanup_older_than")?;

        let rows = stmt
            .query_map(rusqlite::params![cutoff], |row| row.get::<_, String>(0))
            .context("Failed to query aged blobs pending cleanup")?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read blob key")?);
        }
        Ok(result)
    }

    // -- Named locks --

    async fn acquire_named_lock(&mut self, _name: &str) -> Result<()> {
        // No-op for SQLite: BEGIN EXCLUSIVE in start_transaction already serializes writers.
        Ok(())
    }

    async fn release_named_lock(&mut self, _name: &str) -> Result<()> {
        // No-op for SQLite.
        Ok(())
    }

    // -- External ID mappings --

    async fn list_external_id_mappings(
        &mut self,
        ns: &ExternalIDNamespace,
    ) -> Result<Vec<(ExternalID, GraphID)>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT external_id, graph_id FROM {}
             WHERE external_id_namespace = ?1
             ORDER BY graph_id ASC",
            TABLE_EXTERNAL_ID_MAPPINGS
        );
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(rusqlite::params![ns.0], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .map(|r| {
            let (eid, gid) = r.context("Failed to read external_id mapping row")?;
            Ok((ExternalID(eid), GraphID(gid)))
        })
        .collect()
    }

    async fn insert_external_id_mappings(
        &mut self,
        ns: &ExternalIDNamespace,
        mappings: &[(ExternalID, GraphID)],
    ) -> Result<()> {
        let now = Timestamp::now().to_unix_timestamp();
        let conn = self.lock();
        let sql = format!(
            "INSERT INTO {}
             (external_id_namespace, external_id, graph_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            TABLE_EXTERNAL_ID_MAPPINGS
        );
        let mut stmt = conn.prepare(&sql)?;
        for (eid, gid) in mappings {
            stmt.execute(rusqlite::params![ns.0, eid.0, gid.0, now])?;
        }
        Ok(())
    }

    async fn graph_id_to_external_id(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
        graph_id: &GraphID,
    ) -> Result<Option<ExternalID>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT external_id FROM {}
             WHERE external_id_namespace = ?1 AND graph_id = ?2",
            TABLE_EXTERNAL_ID_MAPPINGS
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare graph_id_to_external_id query")?;

        stmt.query_row(
            rusqlite::params![external_id_namespace.0, graph_id.0],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("Failed to query graph_id_to_external_id")?
        .map(|s| Ok(ExternalID(s)))
        .transpose()
    }

    async fn graph_ids_to_external_ids(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
        graph_ids: &[GraphID],
    ) -> Result<Vec<(GraphID, ExternalID)>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT external_id FROM {}
             WHERE external_id_namespace = ?1 AND graph_id = ?2",
            TABLE_EXTERNAL_ID_MAPPINGS
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare graph_ids_to_external_ids query")?;

        let mut result = Vec::new();
        for graph_id in graph_ids {
            let external_id: Option<String> = stmt
                .query_row(
                    rusqlite::params![external_id_namespace.0, graph_id.0],
                    |row| row.get(0),
                )
                .optional()
                .context("Failed to query external_id")?;

            if let Some(eid) = external_id {
                result.push((*graph_id, ExternalID(eid)));
            }
        }
        Ok(result)
    }

    async fn get_latest_external_id(
        &mut self,
        external_id_namespace: &ExternalIDNamespace,
    ) -> Result<Option<ExternalID>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT external_id FROM {}
             WHERE external_id_namespace = ?1
             ORDER BY graph_id DESC
             LIMIT 1",
            TABLE_EXTERNAL_ID_MAPPINGS
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare get_latest_external_id query")?;

        stmt.query_row(rusqlite::params![external_id_namespace.0], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .context("Failed to query latest external_id")?
        .map(|s| Ok(ExternalID(s)))
        .transpose()
    }

    // -- Metric history --

    async fn ensure_metric_history_partitions_exist(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
        node_names: &[String],
    ) -> Result<()> {
        let now = Timestamp::now().to_unix_timestamp();
        let conn = self.lock();
        let sql = format!(
            "INSERT OR IGNORE INTO {}
             (timeline_id, node_name, week_key, data, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            TABLE_METRIC_HISTORY
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("failed to prepare ensure_metric_history_partitions_exist")?;

        let empty_blob: &[u8] = &[];
        for name in node_names {
            stmt.execute(rusqlite::params![
                timeline_id.0,
                name,
                week_key,
                empty_blob,
                now,
            ])
            .context("failed to insert metric_history partition placeholder")?;
        }
        Ok(())
    }

    async fn get_metric_history_for_week(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT node_name, data FROM {}
             WHERE timeline_id = ?1 AND week_key = ?2",
            TABLE_METRIC_HISTORY
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("failed to prepare get_metric_history_for_week")?;

        let rows = stmt
            .query_map(rusqlite::params![timeline_id.0, week_key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .context("failed to query metric_history for week")?;

        let mut result = std::collections::BTreeMap::new();
        for row in rows {
            let (name, data) = row.context("failed to read metric_history row")?;
            if !data.is_empty() {
                result.insert(name, data);
            }
        }
        Ok(result)
    }

    async fn upsert_metric_history_batch(
        &mut self,
        timeline_id: &TimelineID,
        week_key: &str,
        entries: &[(String, Vec<u8>)],
    ) -> Result<()> {
        let now = Timestamp::now().to_unix_timestamp();
        let conn = self.lock();
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (timeline_id, node_name, week_key, data, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            TABLE_METRIC_HISTORY
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("failed to prepare upsert_metric_history_batch")?;

        for (node_name, data) in entries {
            stmt.execute(rusqlite::params![
                timeline_id.0,
                node_name,
                week_key,
                data,
                now,
            ])
            .context("failed to upsert metric_history row")?;
        }
        Ok(())
    }

    async fn get_metric_history_range(
        &mut self,
        timeline_id: &TimelineID,
        node_names: &[String],
        start_week: &str,
        end_week: &str,
    ) -> Result<Vec<(String, String, Vec<u8>)>> {
        if node_names.is_empty() {
            return Ok(vec![]);
        }

        let conn = self.lock();

        let placeholders: Vec<String> = node_names
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 4))
            .collect();

        let sql = format!(
            "SELECT node_name, week_key, data FROM {}
             WHERE timeline_id = ?1 AND week_key >= ?2 AND week_key <= ?3
             AND node_name IN ({})
             AND length(data) > 0
             ORDER BY node_name, week_key",
            TABLE_METRIC_HISTORY,
            placeholders.join(", ")
        );

        let mut stmt = conn
            .prepare(&sql)
            .context("failed to prepare get_metric_history_range")?;

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(timeline_id.0.clone()));
        params.push(Box::new(start_week.to_string()));
        params.push(Box::new(end_week.to_string()));
        for name in node_names {
            params.push(Box::new(name.clone()));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let mut rows = stmt
            .query(param_refs.as_slice())
            .context("failed to query metric_history range")?;

        let mut result = Vec::new();
        while let Some(row) = rows.next().context("failed to read metric_history row")? {
            let node_name: String = row.get(0)?;
            let week_key: String = row.get(1)?;
            let data: Vec<u8> = row.get(2)?;
            result.push((node_name, week_key, data));
        }
        Ok(result)
    }
}

/// Query timeline config from an already-locked connection.
fn query_timeline_config(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
) -> Result<Option<TimelineConfig>> {
    let sql = format!(
        "SELECT config_json FROM {} WHERE timeline_id = ?1",
        TABLE_TIMELINE_CONFIGS
    );
    let mut stmt = conn
        .prepare(&sql)
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

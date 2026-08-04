// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SQL for the plain-row graph metric history tables.
//!
//! These are synchronous helpers taking an already-locked connection; the
//! async `UnigraphGraphConnection` methods in [`crate::graph`] are one-liners
//! over them. Everything here is a thin SQL wrapper — no locking, no
//! transactions, no business rules.
//!
//! ```text
//! graph_history_metrics   timeline_id -> (metric_id <-> metric_name)   tiny dictionary
//! graph_history_status    (timeline_id, graph_id) -> ingest checkpoint
//! graph_history_entries   (timeline_id, node_name, graph_id) -> packed metric blob
//! ```

use std::collections::BTreeMap;
use std::sync::MutexGuard;

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::HistoryEntryRow;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::HistoryStatusRow;
use unigraph_storage_core::TimelineID;
use unigraph_timestamp::Timestamp;

use crate::schema::TABLE_GRAPH_HISTORY_ENTRIES;
use crate::schema::TABLE_GRAPH_HISTORY_METRICS;
use crate::schema::TABLE_GRAPH_HISTORY_STATUS;

/// Max bound parameters per statement. SQLite's default limit is 999; staying
/// well under it keeps room for the leading non-list parameters.
const PARAM_CHUNK: usize = 500;

// -- Metric dictionary --

pub(crate) fn intern_metrics(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    names: &[String],
) -> Result<BTreeMap<String, u32>> {
    let sql = format!(
        "INSERT INTO {TABLE_GRAPH_HISTORY_METRICS} (timeline_id, metric_id, metric_name)
         SELECT ?1,
                COALESCE((SELECT MAX(metric_id) FROM {TABLE_GRAPH_HISTORY_METRICS}
                          WHERE timeline_id = ?1), -1) + 1,
                ?2
         WHERE NOT EXISTS (SELECT 1 FROM {TABLE_GRAPH_HISTORY_METRICS}
                           WHERE timeline_id = ?1 AND metric_name = ?2)"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare intern_history_metrics")?;
    for name in names {
        stmt.execute(rusqlite::params![timeline_id.0, name])
            .context("failed to intern history metric name")?;
    }
    drop(stmt);

    Ok(metric_names(conn, timeline_id)?
        .into_iter()
        .map(|(metric_id, name)| (name, metric_id))
        .collect())
}

pub(crate) fn metric_names(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
) -> Result<BTreeMap<u32, String>> {
    let sql = format!(
        "SELECT metric_id, metric_name FROM {TABLE_GRAPH_HISTORY_METRICS} WHERE timeline_id = ?1"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare get_history_metric_names")?;
    stmt.query_map(rusqlite::params![timeline_id.0], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })
    .context("failed to query history metric names")?
    .map(|row| {
        let (metric_id, name) = row.context("failed to read history metric row")?;
        Ok((
            u32::try_from(metric_id).context("metric_id out of range")?,
            name,
        ))
    })
    .collect()
}

pub(crate) fn delete_metrics(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
) -> Result<u64> {
    let sql = format!("DELETE FROM {TABLE_GRAPH_HISTORY_METRICS} WHERE timeline_id = ?1");
    let deleted = conn
        .execute(&sql, rusqlite::params![timeline_id.0])
        .context("failed to delete history metrics")?;
    Ok(deleted as u64)
}

// -- Entries --

pub(crate) fn insert_entries(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    rows: &[HistoryEntryRow],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "INSERT OR REPLACE INTO {TABLE_GRAPH_HISTORY_ENTRIES}
         (timeline_id, node_name, graph_id, timestamp, metric_values, deferred)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare insert_history_entries")?;
    for row in rows {
        stmt.execute(rusqlite::params![
            timeline_id.0,
            row.node_name,
            row.graph_id.0,
            row.timestamp.to_unix_timestamp(),
            row.values,
            i64::from(row.deferred),
        ])
        .context("failed to insert history entry")?;
    }
    Ok(())
}

/// Most recent value blob strictly before `before_graph_id`, per node.
///
/// One query per chunk rather than one per node: this runs for every frame
/// that introduces new nodes, so a per-node `LIMIT 1` would cost O(nodes)
/// round-trips per frame on wide graphs.
///
/// `MAX(graph_id)` with `GROUP BY node_name` relies on a documented SQLite
/// extension: with exactly one `min()`/`max()` aggregate, the bare columns
/// (here `metric_values`) are taken from the row that produced the extremum,
/// rather than an arbitrary row in the group. See
/// <https://sqlite.org/lang_select.html#bareagg>. It also lets SQLite answer
/// each group with a reverse seek on the `(timeline_id, node_name, graph_id)`
/// primary key instead of scanning every historical row for the node — which
/// a window function over the same predicate would not avoid.
pub(crate) fn last_entries_before(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    before_graph_id: GraphID,
    node_names: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut result = Vec::with_capacity(node_names.len());
    for chunk in node_names.chunks(PARAM_CHUNK) {
        let placeholders = numbered_placeholders(chunk.len(), 3);
        let sql = format!(
            "SELECT node_name, MAX(graph_id), metric_values FROM {TABLE_GRAPH_HISTORY_ENTRIES}
             WHERE timeline_id = ?1 AND graph_id < ?2 AND deferred = 0
             AND node_name IN ({placeholders})
             GROUP BY node_name"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(timeline_id.0.clone()), Box::new(before_graph_id.0)];
        params.extend(
            chunk
                .iter()
                .map(|name| Box::new(name.clone()) as Box<dyn rusqlite::types::ToSql>),
        );

        let mut stmt = conn
            .prepare(&sql)
            .context("failed to prepare get_last_history_entries_before")?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut rows = stmt
            .query(param_refs.as_slice())
            .context("failed to query last history entries")?;
        while let Some(row) = rows.next().context("failed to read last history entry")? {
            result.push((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(2)?));
        }
    }
    Ok(result)
}

pub(crate) fn series(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    node_name: &str,
    range: &HistoryRange,
) -> Result<Vec<(GraphID, Timestamp, Vec<u8>)>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(timeline_id.0.clone()),
        Box::new(node_name.to_string()),
    ];
    let where_clause = history_range_clause(range, &mut params);

    let sql = format!(
        "SELECT graph_id, timestamp, metric_values FROM {TABLE_GRAPH_HISTORY_ENTRIES}
         WHERE timeline_id = ?1 AND node_name = ?2{where_clause}
         ORDER BY graph_id ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare get_history_series")?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut rows = stmt
        .query(param_refs.as_slice())
        .context("failed to query history series")?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().context("failed to read history series row")? {
        result.push((
            GraphID(row.get(0)?),
            Timestamp::from_unix_timestamp(row.get(1)?),
            row.get::<_, Vec<u8>>(2)?,
        ));
    }
    Ok(result)
}

pub(crate) fn node_names(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    range: &HistoryRange,
) -> Result<Vec<String>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = history_range_clause(range, &mut params);

    let sql = format!(
        "SELECT DISTINCT node_name FROM {TABLE_GRAPH_HISTORY_ENTRIES}
         WHERE timeline_id = ?1{where_clause}
         ORDER BY node_name"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare list_history_node_names")?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut rows = stmt
        .query(param_refs.as_slice())
        .context("failed to query history node names")?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().context("failed to read history node name")? {
        result.push(row.get::<_, String>(0)?);
    }
    Ok(result)
}

pub(crate) fn delete_entries(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<u64> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql =
        format!("DELETE FROM {TABLE_GRAPH_HISTORY_ENTRIES} WHERE timeline_id = ?1{where_clause}");
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let deleted = conn
        .execute(&sql, param_refs.as_slice())
        .context("failed to delete history entries")?;
    Ok(deleted as u64)
}

pub(crate) fn delete_entries_for_node(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    node_name: &str,
    graph_ids: &[GraphID],
) -> Result<u64> {
    let mut deleted = 0u64;
    for chunk in graph_ids.chunks(PARAM_CHUNK) {
        let placeholders = numbered_placeholders(chunk.len(), 3);
        let sql = format!(
            "DELETE FROM {TABLE_GRAPH_HISTORY_ENTRIES}
             WHERE timeline_id = ?1 AND node_name = ?2 AND graph_id IN ({placeholders})"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new(timeline_id.0.clone()),
            Box::new(node_name.to_string()),
        ];
        params.extend(
            chunk
                .iter()
                .map(|id| Box::new(id.0) as Box<dyn rusqlite::types::ToSql>),
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        deleted += conn
            .execute(&sql, param_refs.as_slice())
            .context("failed to delete history entries for node")? as u64;
    }
    Ok(deleted)
}

// -- Status --

pub(crate) fn get_status(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_ids: &[GraphID],
) -> Result<Vec<HistoryStatusRow>> {
    let mut result = Vec::new();
    for chunk in graph_ids.chunks(PARAM_CHUNK) {
        let placeholders = numbered_placeholders(chunk.len(), 2);
        let sql = format!(
            "SELECT graph_id, status, attempts, error_blob_key, omission_deferred
             FROM {TABLE_GRAPH_HISTORY_STATUS}
             WHERE timeline_id = ?1 AND graph_id IN ({placeholders})"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(timeline_id.0.clone())];
        params.extend(
            chunk
                .iter()
                .map(|id| Box::new(id.0) as Box<dyn rusqlite::types::ToSql>),
        );

        let mut stmt = conn
            .prepare(&sql)
            .context("failed to prepare get_history_status")?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut rows = stmt
            .query(param_refs.as_slice())
            .context("failed to query history status")?;
        while let Some(row) = rows.next().context("failed to read history status row")? {
            result.push(HistoryStatusRow {
                graph_id: GraphID(row.get(0)?),
                status: row.get(1)?,
                attempts: row.get(2)?,
                error_blob_key: row.get(3)?,
                omission_deferred: row.get::<_, i64>(4)? != 0,
            });
        }
    }
    Ok(result)
}

pub(crate) fn deferred_bounds(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
) -> Result<Option<(GraphID, GraphID)>> {
    let sql = format!(
        "SELECT MIN(graph_id), MAX(graph_id) FROM {TABLE_GRAPH_HISTORY_STATUS}
         WHERE timeline_id = ?1 AND omission_deferred != 0"
    );
    let bounds = conn
        .query_row(&sql, rusqlite::params![timeline_id.0], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .context("failed to query history deferred bounds")?;

    // MIN/MAX over an empty set is one row of NULLs, not zero rows.
    let (Some(min), Some(max)) = bounds else {
        return Ok(None);
    };
    Ok(Some((GraphID(min), GraphID(max))))
}

/// Graph IDs still flagged `omission_deferred` within `bounds`, ascending.
pub(crate) fn deferred_graph_ids(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<Vec<GraphID>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql = format!(
        "SELECT graph_id FROM {TABLE_GRAPH_HISTORY_STATUS}
         WHERE timeline_id = ?1 AND omission_deferred != 0{where_clause}
         ORDER BY graph_id ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare list_history_deferred_graph_ids")?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut rows = stmt
        .query(param_refs.as_slice())
        .context("failed to query history deferred graph ids")?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .context("failed to read history deferred graph id")?
    {
        result.push(GraphID(row.get(0)?));
    }
    Ok(result)
}

pub(crate) fn entries_at(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_id: GraphID,
) -> Result<Vec<(String, Vec<u8>)>> {
    let sql = format!(
        "SELECT node_name, metric_values FROM {TABLE_GRAPH_HISTORY_ENTRIES}
         WHERE timeline_id = ?1 AND graph_id = ?2
         ORDER BY node_name"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare get_history_entries_at")?;
    let mut rows = stmt
        .query(rusqlite::params![timeline_id.0, graph_id.0])
        .context("failed to query history entries at graph id")?;

    let mut result = Vec::new();
    while let Some(row) = rows.next().context("failed to read history entry")? {
        result.push((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?));
    }
    Ok(result)
}

pub(crate) fn delete_entries_at(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_id: GraphID,
    node_names: &[String],
) -> Result<u64> {
    let mut deleted = 0u64;
    for chunk in node_names.chunks(PARAM_CHUNK) {
        let placeholders = numbered_placeholders(chunk.len(), 3);
        let sql = format!(
            "DELETE FROM {TABLE_GRAPH_HISTORY_ENTRIES}
             WHERE timeline_id = ?1 AND graph_id = ?2 AND node_name IN ({placeholders})"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(timeline_id.0.clone()), Box::new(graph_id.0)];
        params.extend(
            chunk
                .iter()
                .map(|name| Box::new(name.clone()) as Box<dyn rusqlite::types::ToSql>),
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        deleted += conn
            .execute(&sql, param_refs.as_slice())
            .context("failed to delete history entries at graph id")? as u64;
    }
    Ok(deleted)
}

pub(crate) fn clear_entries_deferred(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<u64> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql = format!(
        "UPDATE {TABLE_GRAPH_HISTORY_ENTRIES} SET deferred = 0
         WHERE timeline_id = ?1 AND deferred != 0{where_clause}"
    );
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let updated = conn
        .execute(&sql, param_refs.as_slice())
        .context("failed to clear history entry deferred flags")?;
    Ok(updated as u64)
}

pub(crate) fn clear_omission_deferred(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<u64> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql = format!(
        "UPDATE {TABLE_GRAPH_HISTORY_STATUS} SET omission_deferred = 0
         WHERE timeline_id = ?1 AND omission_deferred != 0{where_clause}"
    );
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let updated = conn
        .execute(&sql, param_refs.as_slice())
        .context("failed to clear history omission_deferred")?;
    Ok(updated as u64)
}

pub(crate) fn upsert_status(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    rows: &[HistoryStatusRow],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let now = Timestamp::now().to_unix_timestamp();
    let sql = format!(
        "INSERT OR REPLACE INTO {TABLE_GRAPH_HISTORY_STATUS}
         (timeline_id, graph_id, status, attempts, error_blob_key, omission_deferred, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare upsert_history_status")?;
    for row in rows {
        stmt.execute(rusqlite::params![
            timeline_id.0,
            row.graph_id.0,
            row.status,
            row.attempts,
            row.error_blob_key,
            i64::from(row.omission_deferred),
            now,
        ])
        .context("failed to upsert history status")?;
    }
    Ok(())
}

pub(crate) fn error_blob_keys(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<Vec<String>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql = format!(
        "SELECT error_blob_key FROM {TABLE_GRAPH_HISTORY_STATUS}
         WHERE timeline_id = ?1 AND error_blob_key IS NOT NULL{where_clause}
         ORDER BY graph_id"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare get_history_error_blob_keys")?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut rows = stmt
        .query(param_refs.as_slice())
        .context("failed to query history error blob keys")?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .context("failed to read history error blob key")?
    {
        result.push(row.get::<_, String>(0)?);
    }
    Ok(result)
}

pub(crate) fn delete_status(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<u64> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql =
        format!("DELETE FROM {TABLE_GRAPH_HISTORY_STATUS} WHERE timeline_id = ?1{where_clause}");
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let deleted = conn
        .execute(&sql, param_refs.as_slice())
        .context("failed to delete history status")?;
    Ok(deleted as u64)
}

// -- Shared clause builders --

/// Append `timestamp` and `graph_id` bound params and return the matching SQL
/// fragment (empty when the range is unbounded).
fn history_range_clause(
    range: &HistoryRange,
    params: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
) -> String {
    let mut clause = String::new();
    if let Some(start) = &range.timestamps.start {
        params.push(Box::new(start.to_unix_timestamp()));
        clause.push_str(&format!(" AND timestamp >= ?{}", params.len()));
    }
    if let Some(end) = &range.timestamps.end {
        params.push(Box::new(end.to_unix_timestamp()));
        clause.push_str(&format!(" AND timestamp <= ?{}", params.len()));
    }
    clause.push_str(&graph_id_bounds_clause(&range.graph_ids, params));
    clause
}

/// Append `graph_id` bound params and return the matching SQL fragment
/// (empty when both bounds are `None`).
fn graph_id_bounds_clause(
    bounds: &GraphIDBounds,
    params: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
) -> String {
    let mut clause = String::new();
    if let Some(from) = &bounds.0 {
        params.push(Box::new(from.0));
        clause.push_str(&format!(" AND graph_id >= ?{}", params.len()));
    }
    if let Some(to) = &bounds.1 {
        params.push(Box::new(to.0));
        clause.push_str(&format!(" AND graph_id <= ?{}", params.len()));
    }
    clause
}

/// `?N, ?N+1, ...` for `count` params starting at index `start`.
fn numbered_placeholders(count: usize, start: usize) -> String {
    (start..start + count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

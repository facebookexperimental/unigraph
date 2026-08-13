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
//! graph_history_status    (timeline_id, graph_id) -> ingest checkpoint + gap flags
//! graph_history_entries   (timeline_id, node_name, graph_id) -> packed metric blob + reasons
//! ```
//!
//! `reasons` and `frame_flags` are bitmasks whose meaning lives in
//! `unigraph_db::graph_history`. The updates below are expressed in SQL rather
//! than read-modify-write in Rust so that adding one reason to a row cannot
//! clobber another writer's — and so that bits this binary does not recognise
//! survive untouched.

use std::collections::BTreeMap;
use std::sync::MutexGuard;

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use unigraph_storage_core::ExclusiveGraphIDRange;
use unigraph_storage_core::FrameFlags;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::HistoryEntryRow;
use unigraph_storage_core::HistoryNodeSample;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::HistorySampleRow;
use unigraph_storage_core::HistoryStatusRow;
use unigraph_storage_core::IngestState;
use unigraph_storage_core::Reasons;
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
         (timeline_id, node_name, graph_id, timestamp, metric_values, reasons)
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
            i64::from(row.reasons.bits()),
        ])
        .context("failed to insert history entry")?;
    }
    Ok(())
}

pub(crate) fn series(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    node_name: &str,
    range: &HistoryRange,
) -> Result<Vec<HistorySampleRow>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(timeline_id.0.clone()),
        Box::new(node_name.to_string()),
    ];
    let where_clause = history_range_clause(range, &mut params);

    let sql = format!(
        "SELECT graph_id, timestamp, metric_values, reasons FROM {TABLE_GRAPH_HISTORY_ENTRIES}
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
        result.push(HistorySampleRow {
            graph_id: GraphID(row.get(0)?),
            timestamp: Timestamp::from_unix_timestamp(row.get(1)?),
            values: row.get::<_, Vec<u8>>(2)?,
            reasons: Reasons::from_bits_retain(bitmask_from_sql(row.get::<_, i64>(3)?)?),
        });
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

pub(crate) fn entries_at(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_id: GraphID,
) -> Result<Vec<HistoryNodeSample>> {
    let sql = format!(
        "SELECT node_name, metric_values, reasons FROM {TABLE_GRAPH_HISTORY_ENTRIES}
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
        result.push(HistoryNodeSample {
            node_name: row.get::<_, String>(0)?,
            values: row.get::<_, Vec<u8>>(1)?,
            reasons: Reasons::from_bits_retain(bitmask_from_sql(row.get::<_, i64>(2)?)?),
        });
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

/// Every zero-reason row strictly between the bounds, for all nodes.
///
/// One range statement per segment rather than one per node: the bounds are
/// barrier frames, whose rows are held by their frame flags, so nothing inside
/// the interval needs protecting and there is nothing to join against.
pub(crate) fn delete_collapsed_entries(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    segment: &ExclusiveGraphIDRange,
) -> Result<u64> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let mut clause = String::new();
    if let Some(after) = segment.after {
        params.push(Box::new(after.0));
        clause.push_str(&format!(" AND graph_id > ?{}", params.len()));
    }
    if let Some(before) = segment.before {
        params.push(Box::new(before.0));
        clause.push_str(&format!(" AND graph_id < ?{}", params.len()));
    }

    let sql = format!(
        "DELETE FROM {TABLE_GRAPH_HISTORY_ENTRIES}
         WHERE timeline_id = ?1 AND reasons = 0{clause}"
    );
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let deleted = conn
        .execute(&sql, param_refs.as_slice())
        .context("failed to delete collapsed history entries")?;
    Ok(deleted as u64)
}

/// OR in `set` and mask out `clear` for the given nodes at one graph ID.
///
/// Done in SQL so that adding `ANCHOR` to a row cannot silently drop whatever
/// else that row is — under this design a row is routinely a crossing *and* an
/// anchor at the same time.
pub(crate) fn set_reasons_at(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_id: GraphID,
    node_names: &[String],
    set: Reasons,
    clear: Reasons,
) -> Result<u64> {
    let mut updated = 0u64;
    for chunk in node_names.chunks(PARAM_CHUNK) {
        let placeholders = numbered_placeholders(chunk.len(), 5);
        let sql = format!(
            "UPDATE {TABLE_GRAPH_HISTORY_ENTRIES} SET reasons = (reasons | ?3) & ~?4
             WHERE timeline_id = ?1 AND graph_id = ?2 AND node_name IN ({placeholders})"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new(timeline_id.0.clone()),
            Box::new(graph_id.0),
            Box::new(i64::from(set.bits())),
            Box::new(i64::from(clear.bits())),
        ];
        params.extend(
            chunk
                .iter()
                .map(|name| Box::new(name.clone()) as Box<dyn rusqlite::types::ToSql>),
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        updated += conn
            .execute(&sql, param_refs.as_slice())
            .context("failed to update history entry reasons")? as u64;
    }
    Ok(updated)
}

pub(crate) fn clear_reasons_at(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_id: GraphID,
    clear: Reasons,
) -> Result<u64> {
    let sql = format!(
        "UPDATE {TABLE_GRAPH_HISTORY_ENTRIES} SET reasons = reasons & ~?3
         WHERE timeline_id = ?1 AND graph_id = ?2 AND reasons & ?3 != 0"
    );
    let updated = conn
        .execute(
            &sql,
            rusqlite::params![timeline_id.0, graph_id.0, i64::from(clear.bits())],
        )
        .context("failed to clear history entry reasons")?;
    Ok(updated as u64)
}

pub(crate) fn set_entry_reasons(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    node_name: &str,
    rows: &[(GraphID, Reasons)],
) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }

    let sql = format!(
        "UPDATE {TABLE_GRAPH_HISTORY_ENTRIES} SET reasons = ?4
         WHERE timeline_id = ?1 AND node_name = ?2 AND graph_id = ?3"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare set_history_entry_reasons")?;
    let mut updated = 0u64;
    for (graph_id, reasons) in rows {
        updated += stmt
            .execute(rusqlite::params![
                timeline_id.0,
                node_name,
                graph_id.0,
                i64::from(reasons.bits()),
            ])
            .context("failed to set history entry reasons")? as u64;
    }
    Ok(updated)
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
            "SELECT graph_id, ingest_state, attempts, error_blob_key, frame_flags
             FROM {TABLE_GRAPH_HISTORY_STATUS}
             WHERE timeline_id = ?1 AND graph_id IN ({placeholders})
             ORDER BY graph_id ASC"
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
            result.push(read_status(row)?);
        }
    }
    Ok(result)
}

pub(crate) fn list_statuses(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    bounds: &GraphIDBounds,
) -> Result<Vec<HistoryStatusRow>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(timeline_id.0.clone())];
    let where_clause = graph_id_bounds_clause(bounds, &mut params);

    let sql = format!(
        "SELECT graph_id, ingest_state, attempts, error_blob_key, frame_flags
         FROM {TABLE_GRAPH_HISTORY_STATUS}
         WHERE timeline_id = ?1{where_clause}
         ORDER BY graph_id ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare list_history_statuses")?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut rows = stmt
        .query(param_refs.as_slice())
        .context("failed to query history statuses")?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().context("failed to read history status row")? {
        result.push(read_status(row)?);
    }
    Ok(result)
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
         (timeline_id, graph_id, ingest_state, attempts, error_blob_key, frame_flags, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare upsert_history_status")?;
    for row in rows {
        stmt.execute(rusqlite::params![
            timeline_id.0,
            row.graph_id.0,
            row.ingest_state.to_string(),
            row.attempts,
            row.error_blob_key,
            i64::from(row.frame_flags.bits()),
            now,
        ])
        .context("failed to upsert history status")?;
    }
    Ok(())
}

pub(crate) fn set_frame_flags(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    rows: &[(GraphID, FrameFlags)],
) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }

    let now = Timestamp::now().to_unix_timestamp();
    let sql = format!(
        "UPDATE {TABLE_GRAPH_HISTORY_STATUS} SET frame_flags = ?3, updated_at = ?4
         WHERE timeline_id = ?1 AND graph_id = ?2"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("failed to prepare set_history_frame_flags")?;
    let mut updated = 0u64;
    for (graph_id, flags) in rows {
        updated += stmt
            .execute(rusqlite::params![
                timeline_id.0,
                graph_id.0,
                i64::from(flags.bits()),
                now,
            ])
            .context("failed to set history frame flags")? as u64;
    }
    Ok(updated)
}

pub(crate) fn set_ingest_states(
    conn: &MutexGuard<'_, Connection>,
    timeline_id: &TimelineID,
    graph_ids: &[GraphID],
    ingest_state: IngestState,
) -> Result<u64> {
    let now = Timestamp::now().to_unix_timestamp();
    let mut updated = 0u64;
    for chunk in graph_ids.chunks(PARAM_CHUNK) {
        let placeholders = numbered_placeholders(chunk.len(), 4);
        let sql = format!(
            "UPDATE {TABLE_GRAPH_HISTORY_STATUS} SET ingest_state = ?2, updated_at = ?3
             WHERE timeline_id = ?1 AND graph_id IN ({placeholders})"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new(timeline_id.0.clone()),
            Box::new(ingest_state.to_string()),
            Box::new(now),
        ];
        params.extend(
            chunk
                .iter()
                .map(|id| Box::new(id.0) as Box<dyn rusqlite::types::ToSql>),
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        updated += conn
            .execute(&sql, param_refs.as_slice())
            .context("failed to set history ingest state")? as u64;
    }
    Ok(updated)
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

// -- Shared row readers and clause builders --

fn read_status(row: &rusqlite::Row<'_>) -> Result<HistoryStatusRow> {
    let ingest_state = row.get::<_, String>(1)?;
    Ok(HistoryStatusRow {
        graph_id: GraphID(row.get(0)?),
        ingest_state: ingest_state.parse().with_context(|| {
            format!("unreadable graph_history_status.ingest_state: {ingest_state}")
        })?,
        attempts: row.get(2)?,
        error_blob_key: row.get(3)?,
        frame_flags: FrameFlags::from_bits_retain(bitmask_from_sql(row.get::<_, i64>(4)?)?),
    })
}

/// Bitmask columns are `INTEGER` in SQLite, which is signed. A value that does
/// not fit a `u32` means something other than this code wrote it.
///
/// Unknown *bits* are a different matter and are kept — see the `from_bits_retain`
/// calls above.
fn bitmask_from_sql(value: i64) -> Result<u32> {
    u32::try_from(value).context("history bitmask column is out of range for u32")
}

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

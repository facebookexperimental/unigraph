// Copyright (c) Meta Platforms, Inc. and affiliates.

//! External ID mapping — allocate, look up, and manage ExternalID ↔ GraphID mappings.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use unigraph_storage_core::ExternalID;
use unigraph_storage_core::ExternalIDNamespace;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::UnigraphGraphConnection;

use crate::storage::UnigraphStorage;

/// Handle for external ID mapping operations.
///
/// Obtained via [`UnigraphDb::external_ids`](crate::UnigraphDb).
#[derive(Clone)]
pub struct ExternalIds {
    pub(crate) storage: Arc<UnigraphStorage>,
}

// -- Public API --

impl ExternalIds {
    /// Register new ExternalIDs and allocate sequential GraphIDs.
    ///
    /// Manages lock + transaction internally:
    /// 1. Acquires a named lock for the namespace
    /// 2. Starts an exclusive transaction
    /// 3. Loads existing mappings (may have moved since caller checked)
    /// 4. Validates linear history (overlapping prefix must be contiguous)
    /// 5. Allocates sequential GraphIDs for the new tail
    /// 6. Commits the transaction and releases the lock
    ///
    /// Returns GraphIDs for ALL input ExternalIDs (both pre-existing and
    /// newly allocated), in the same order as the input.
    pub async fn add_new(
        &self,
        ns: &ExternalIDNamespace,
        external_ids: &[ExternalID],
    ) -> Result<Vec<GraphID>> {
        if external_ids.is_empty() {
            return Ok(vec![]);
        }

        let lock_name = format!("external_ids:{}", ns.0);
        let mut conn = self.storage.graph.conn().await?;
        conn.acquire_named_lock(&lock_name).await?;
        conn.start_transaction().await?;

        let result = resolve_and_allocate(&mut *conn, ns, external_ids).await;

        finish_transaction(&mut *conn, result, &lock_name).await
    }

    /// Look up the ExternalID for a GraphID within a namespace.
    pub async fn to_external_id(
        &self,
        ns: &ExternalIDNamespace,
        graph_id: &GraphID,
    ) -> Result<Option<ExternalID>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.graph_id_to_external_id(ns, graph_id).await
    }

    /// Look up ExternalIDs for multiple GraphIDs within a namespace (batch).
    pub async fn to_external_ids(
        &self,
        ns: &ExternalIDNamespace,
        graph_ids: &[GraphID],
    ) -> Result<Vec<(GraphID, ExternalID)>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.graph_ids_to_external_ids(ns, graph_ids).await
    }

    /// Get the ExternalID with the highest GraphID in a namespace.
    pub async fn get_latest(&self, ns: &ExternalIDNamespace) -> Result<Option<ExternalID>> {
        let mut conn = self.storage.graph.conn().await?;
        conn.get_latest_external_id(ns).await
    }
}

// -- Private helpers for allocation --

/// Load existing mappings, validate linear history, and insert new ones.
/// Must be called inside an exclusive transaction.
async fn resolve_and_allocate(
    conn: &mut dyn UnigraphGraphConnection,
    ns: &ExternalIDNamespace,
    external_ids: &[ExternalID],
) -> Result<Vec<GraphID>> {
    let existing = conn.list_external_id_mappings(ns).await?;
    let existing_map: HashMap<&str, GraphID> = existing
        .iter()
        .map(|(eid, gid)| (eid.0.as_str(), *gid))
        .collect();

    let skip_count = count_overlap_prefix(external_ids, &existing_map);
    validate_no_gaps(external_ids, skip_count, &existing_map)?;

    let prefix_ids = resolve_prefix(external_ids, skip_count, &existing_map);
    let new_mappings = allocate_new_tail(external_ids, skip_count, &existing);
    conn.insert_external_id_mappings(ns, &new_mappings).await?;

    let new_ids: Vec<GraphID> = new_mappings.into_iter().map(|(_, gid)| gid).collect();
    Ok([prefix_ids, new_ids].concat())
}

/// Commit on success, rollback on error, release lock in both cases.
async fn finish_transaction<T>(
    conn: &mut dyn UnigraphGraphConnection,
    result: Result<T>,
    lock_name: &str,
) -> Result<T> {
    match result {
        Ok(val) => {
            conn.commit_transaction().await?;
            conn.release_named_lock(lock_name).await?;
            Ok(val)
        }
        Err(e) => {
            // Transaction rolls back on connection drop.
            conn.release_named_lock(lock_name).await?;
            Err(e)
        }
    }
}

/// Count how many external_ids from the front already exist (contiguous prefix).
fn count_overlap_prefix(external_ids: &[ExternalID], existing: &HashMap<&str, GraphID>) -> usize {
    external_ids
        .iter()
        .take_while(|eid| existing.contains_key(eid.0.as_str()))
        .count()
}

/// Verify no external_id after the overlap prefix already exists.
fn validate_no_gaps(
    external_ids: &[ExternalID],
    skip_count: usize,
    existing: &HashMap<&str, GraphID>,
) -> Result<()> {
    for eid in &external_ids[skip_count..] {
        if existing.contains_key(eid.0.as_str()) {
            anyhow::bail!(
                "Non-linear history: external_id '{}' already exists but appears after \
                 new external_ids in the input list. The overlapping prefix must be contiguous.",
                eid.0
            );
        }
    }
    Ok(())
}

/// Resolve the overlapping prefix to their existing GraphIDs.
fn resolve_prefix(
    external_ids: &[ExternalID],
    skip_count: usize,
    existing: &HashMap<&str, GraphID>,
) -> Vec<GraphID> {
    external_ids[..skip_count]
        .iter()
        .map(|eid| existing[eid.0.as_str()])
        .collect()
}

/// Build (ExternalID, GraphID) pairs for the new tail, starting after the
/// highest existing graph_id.
fn allocate_new_tail(
    external_ids: &[ExternalID],
    skip_count: usize,
    existing: &[(ExternalID, GraphID)],
) -> Vec<(ExternalID, GraphID)> {
    let mut next_id = existing.last().map(|(_, gid)| gid.0).unwrap_or(0) + 1;
    external_ids[skip_count..]
        .iter()
        .map(|eid| {
            let gid = GraphID(next_id);
            next_id += 1;
            (eid.clone(), gid)
        })
        .collect()
}

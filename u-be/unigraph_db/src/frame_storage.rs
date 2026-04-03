// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Frame store/fetch/delete operations for [`UnigraphStorage`].
//!
//! Handles packing graphs into manifests + blobs, deciding between inline
//! and external blob storage, transactional frame writes with timeline
//! locking, reconstructing graphs from delta chains, and frame deletion
//! with safe blob cleanup.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializableManifest;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::BlobID;
use unigraph_core::DeltaManifest;
use unigraph_core::DeltaPackage;
use unigraph_core::ErrorManifest;
use unigraph_core::ErrorPackage;
use unigraph_core::pack_errors;
use unigraph_core::unpack_delta;
use unigraph_core::unpack_errors;
use unigraph_serialization::ZSTDCompressionLevel;
use unigraph_serialization::from_zstd;
use unigraph_serialization::to_zstd;
use unigraph_storage_core::FrameData;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampedError;
use unigraph_storage_core::UnigraphGraphConnection;

use crate::storage::UnigraphStorage;

/// Pre-processed blobs ready for storage — either compressed inline bytes
/// or external blob keys (already uploaded).
pub(crate) struct PreparedBlobs {
    /// Compressed inline bytes (if total size ≤ threshold).
    pub inline: Option<Vec<u8>>,
    /// External blob keys (if uploaded to external storage).
    /// These need to be unregistered from the cleanup table inside the
    /// frame transaction after a successful write.
    pub external_keys: Option<Vec<String>>,
}

impl UnigraphStorage {
    /// Store error data for a failed graph computation.
    ///
    /// Schema-agnostic — no validation, no history.
    pub async fn store_error(
        &self,
        key: &GraphTimeKey,
        errors: &[TimestampedError],
        config: &ArrayGraphSerializablePackageConfig,
        task: &ll::Task,
    ) -> Result<()> {
        let error_count = errors.len() as u32;
        let errors_vec = errors.to_vec();
        let package =
            pack_errors(&errors_vec, error_count, config).context("Failed to pack errors")?;
        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize error manifest")?;

        let prepared = self
            .prepare_blobs_for_storage(&key.timeline_id, &package.blobs, task)
            .await?;

        let mut conn = self.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(&key.timeline_id, task)
            .await?;

        self.store_package_on_conn(
            &mut *conn,
            key,
            FrameType::Error,
            None,
            &manifest_json,
            prepared.inline.as_deref(),
            prepared.external_keys.as_deref(),
            None,
            task,
        )
        .await?;

        conn.commit_transaction(task).await?;
        Ok(())
    }

    /// Fetch and reconstruct a graph from storage.
    ///
    /// Dispatches to the appropriate fetch strategy based on the timeline schema.
    ///
    /// Returns a boxed future because FullOrDelta's cross-timeline delta
    /// chain walker can recurse back into this method (the base graph may
    /// live in a different timeline with a different schema).
    pub fn fetch_graph(
        &self,
        key: &GraphKey,
        task: &ll::Task,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ArrayGraphSerializable>> + Send + '_>,
    > {
        let key = key.clone();
        let task = task.clone();
        Box::pin(async move {
            let schema = {
                let mut conn = self.graph.conn().await?;
                let config = conn
                    .get_timeline_config(&key.timeline_id, &task)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", key.timeline_id))?;
                config.schema
            };
            match schema {
                unigraph_storage_core::TimelineSchema::AdjacentDeltas(_) => {
                    crate::schemas::adjacent_deltas::fetch_graph(self, &key, &task).await
                }
                unigraph_storage_core::TimelineSchema::FullOrDelta(_) => {
                    crate::schemas::full_or_delta::fetch_graph(self, &key, &task).await
                }
            }
        })
    }

    /// Fetch errors for a frame.
    pub async fn fetch_errors(
        &self,
        key: &GraphKey,
        task: &ll::Task,
    ) -> Result<Vec<TimestampedError>> {
        let mut conn = self.graph.conn().await?;
        let row = get_frame_with_data(&mut *conn, key, task).await?;

        if row.frame_type != FrameType::Error {
            anyhow::bail!("Frame {:?} is {:?}, not Error", key, row.frame_type);
        }

        let data = row
            .data
            .ok_or_else(|| anyhow::anyhow!("Frame data missing for {:?}", key))?;

        let manifest: ErrorManifest =
            serde_json::from_str(&data.manifest_json).context("Failed to parse ErrorManifest")?;

        let blobs = self
            .resolve_blobs(&manifest.errors_blob, data.inline_blobs.as_deref(), task)
            .await?;

        let package = ErrorPackage { manifest, blobs };
        unpack_errors(&package).context("Failed to unpack errors")
    }

    /// Delete a frame and register its external blobs for cleanup.
    ///
    /// **Caller must provide a connection that is already inside a transaction.**
    /// This method does NOT start/commit a transaction — the caller controls that.
    ///
    /// Steps:
    /// 1. Fetch the frame (with data) to read the manifest
    /// 2. Extract external blob keys from the manifest
    /// 3. Register blob keys for cleanup (so the sweeper will delete them)
    /// 4. Delete the frame row
    ///
    /// Returns `true` if the frame existed and was deleted.
    pub async fn delete_frame_on_conn(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &GraphKey,
        task: &ll::Task,
    ) -> Result<bool> {
        let row = match get_frame_with_data_on_conn(conn, key, task).await? {
            Some(row) => row,
            None => return Ok(false),
        };

        let blob_keys = extract_external_blob_keys(&row)?;
        if !blob_keys.is_empty() {
            conn.register_blobs_for_cleanup(&blob_keys, task).await?;
        }

        conn.delete_frame(key, task).await
    }

    /// Sweep external blobs that have been pending cleanup for at least `min_age`.
    ///
    /// Steps:
    /// 1. Query `blobs_to_delete` for entries older than `now - min_age`
    /// 2. Delete each blob from external blob storage
    /// 3. Unregister the blob keys from the cleanup table
    ///
    /// Returns the number of blobs swept.
    pub async fn sweep_blobs(
        &self,
        min_age: std::time::Duration,
        task: &ll::Task,
    ) -> Result<usize> {
        let now = Timestamp::now().to_unix_timestamp();
        let cutoff = Timestamp::from_unix_timestamp(now - min_age.as_secs() as i64);

        let mut conn = self.graph.conn().await?;
        let blob_keys = conn
            .get_blobs_pending_cleanup_older_than(cutoff, task)
            .await?;
        drop(conn);

        if blob_keys.is_empty() {
            return Ok(0);
        }

        // Delete from external blob storage (outside any transaction).
        for key in &blob_keys {
            self.blob.delete_blob(key).await?;
        }

        // Unregister from cleanup table (separate short-lived connection).
        let mut conn = self.graph.conn_write().await?;
        conn.unregister_blobs_for_cleanup(&blob_keys, task).await?;

        Ok(blob_keys.len())
    }

    // --- internal helpers ---

    /// Prepare metric history if enabled for this timeline.
    ///
    /// Reads the timeline config, checks `store_metric_history`, extracts
    /// metrics from the graph, and ensures partition rows exist in the DB.
    /// Must be called BEFORE the transaction (MySQL row-locking bug workaround).
    ///
    /// Returns `None` if history is disabled or the timeline doesn't exist.
    pub(crate) async fn prepare_history_if_enabled(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
        task: &ll::Task,
    ) -> Result<Option<crate::metric_history::PreparedHistoryEntries>> {
        let config = {
            let mut conn = self.graph.conn().await?;
            conn.get_timeline_config(&key.timeline_id, task).await?
        };

        let Some(config) = config else {
            return Ok(None);
        };

        if config.store_metric_history != Some(true) {
            return Ok(None);
        }

        let prepared = crate::metric_history::prepare_history_entries(&[(key.clone(), graph)]);
        let mut conn = self.graph.conn_write().await?;
        crate::metric_history::ensure_history_partitions(
            &mut *conn,
            &key.timeline_id,
            &prepared,
            task,
        )
        .await?;

        Ok(Some(prepared))
    }

    /// Store a packed package on an existing connection.
    ///
    /// **Caller must provide a connection that is already inside a transaction.**
    ///
    /// If blobs are external, they must have already been uploaded and their
    /// keys registered for cleanup BEFORE calling this. Pass those keys in
    /// `blob_keys_to_unregister` so they get unregistered inside the transaction.
    #[allow(clippy::too_many_arguments)]
    pub async fn store_package_on_conn(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
        blob_keys_to_unregister: Option<&[String]>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()> {
        conn.store_frame(
            key,
            frame_type,
            base,
            manifest_json,
            inline_blobs,
            expires_at,
            task,
        )
        .await?;

        // Unregister blob keys from cleanup table INSIDE the transaction,
        // so if commit fails the blobs remain registered for cleanup.
        if let Some(blob_keys) = blob_keys_to_unregister {
            conn.unregister_blobs_for_cleanup(blob_keys, task).await?;
        }

        Ok(())
    }

    /// Reconstruct a full graph from frame data.
    pub(crate) async fn reconstruct_full_graph(
        &self,
        data: &FrameData,
        task: &ll::Task,
    ) -> Result<ArrayGraphSerializable> {
        let manifest: ArrayGraphSerializableManifest = serde_json::from_str(&data.manifest_json)
            .context("Failed to parse ArrayGraphSerializableManifest")?;

        let all_blob_ids = manifest.blobs.get_all_blob_ids();
        let blobs = self
            .resolve_blobs(&all_blob_ids, data.inline_blobs.as_deref(), task)
            .await?;

        // Also insert the manifest blob itself so unpack can find it
        let mut blobs_with_manifest = blobs;
        blobs_with_manifest.insert(
            manifest.self_reference.clone(),
            data.manifest_json.as_bytes().to_vec(),
        );

        let package = ArrayGraphSerializablePackage {
            manifest,
            blobs: blobs_with_manifest,
        };

        ArrayGraphSerializable::unpack(&package).context("Failed to unpack graph")
    }

    /// Reconstruct a delta from frame data.
    pub(crate) async fn reconstruct_delta(
        &self,
        data: &FrameData,
        task: &ll::Task,
    ) -> Result<unigraph_core::GraphDelta> {
        let manifest: DeltaManifest =
            serde_json::from_str(&data.manifest_json).context("Failed to parse DeltaManifest")?;

        let blobs = self
            .resolve_blobs(&manifest.delta_blob, data.inline_blobs.as_deref(), task)
            .await?;

        let package = DeltaPackage { manifest, blobs };
        unpack_delta(&package).context("Failed to unpack delta")
    }

    /// Resolve blobs either from inline data or external blob storage.
    ///
    /// Blob IDs already contain the full storage key (including the
    /// `timeline_id/graph_id/` prefix), so they are used directly as
    /// external blob keys.
    #[ll::task]
    pub(crate) async fn resolve_blobs(
        &self,
        blob_ids: &[BlobID],
        inline_blobs: Option<&[u8]>,
        _task: &ll::Task,
    ) -> Result<BTreeMap<BlobID, Vec<u8>>> {
        if let Some(compressed) = inline_blobs {
            // Inline: decompress → deserialize BTreeMap<BlobID, Vec<u8>>
            let decompressed =
                from_zstd(compressed).context("Failed to decompress inline blobs")?;
            let all_blobs: BTreeMap<BlobID, Vec<u8>> = serde_json::from_slice(&decompressed)
                .context("Failed to deserialize inline blobs map")?;
            Ok(all_blobs)
        } else {
            // External: fetch each blob from blob storage
            let mut result = BTreeMap::new();
            for blob_id in blob_ids {
                let data = self
                    .blob
                    .get_blob(&blob_id.0)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Missing external blob: {}", blob_id.0))?;
                result.insert(blob_id.clone(), data);
            }
            Ok(result)
        }
    }

    /// Register blob keys for cleanup and upload blobs to external storage.
    ///
    /// Uses a separate short-lived connection for the cleanup registration
    /// so it doesn't interfere with the caller's transaction.
    ///
    /// Returns the list of blob keys so the caller can unregister them
    /// inside the frame transaction after a successful write.
    pub async fn upload_blobs(
        &self,
        blobs: &BTreeMap<BlobID, Vec<u8>>,
        task: &ll::Task,
    ) -> Result<Vec<String>> {
        let blob_keys: Vec<String> = blobs.keys().map(|id| id.0.clone()).collect();

        // Register for cleanup using a separate short-lived connection.
        let mut reg_conn = self.graph.conn_write().await?;
        reg_conn
            .register_blobs_for_cleanup(&blob_keys, task)
            .await?;
        drop(reg_conn);

        // Upload blobs (outside any transaction).
        for (blob_id, data) in blobs {
            self.blob
                .put_blob(&blob_id.0, data)
                .await
                .with_context(|| format!("Failed to upload blob: {}", blob_id.0))?;
        }

        Ok(blob_keys)
    }

    /// Look up the timeline's inline threshold, prepare inline bytes or upload
    /// to external storage. Returns a [`PreparedBlobs`] ready for
    /// `store_package_on_conn`.
    pub(crate) async fn prepare_blobs_for_storage(
        &self,
        timeline_id: &unigraph_storage_core::TimelineID,
        blobs: &BTreeMap<BlobID, Vec<u8>>,
        task: &ll::Task,
    ) -> Result<PreparedBlobs> {
        let threshold = {
            let mut conn = self.graph.conn().await?;
            let config = conn
                .get_timeline_config(timeline_id, task)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
            config.inline_blob_threshold()
        };

        let inline = prepare_inline_blobs(blobs, threshold)?;
        let external_keys = if inline.is_none() {
            Some(self.upload_blobs(blobs, task).await?)
        } else {
            None
        };

        Ok(PreparedBlobs {
            inline,
            external_keys,
        })
    }

    /// Fetch a single frame with data, or error if not found.
    ///
    /// Convenience wrapper around `select_frames` for use by schema modules.
    pub(crate) async fn get_frame_with_data(
        &self,
        key: &GraphKey,
        task: &ll::Task,
    ) -> Result<FrameRow> {
        let mut conn = self.graph.conn().await?;
        get_frame_with_data(&mut *conn, key, task).await
    }
}

/// Check total blob size and return compressed inline bytes if under the
/// threshold, or `None` if blobs should be stored externally.
///
/// Use `TimelineConfig::inline_blob_threshold()` to get the threshold for
/// a specific timeline, or `DEFAULT_INLINE_BLOB_THRESHOLD_BYTES` for the default.
pub(crate) fn prepare_inline_blobs(
    blobs: &BTreeMap<BlobID, Vec<u8>>,
    inline_threshold_bytes: usize,
) -> Result<Option<Vec<u8>>> {
    let total_size_bytes: usize = blobs.values().map(|b| b.len()).sum();

    if total_size_bytes <= inline_threshold_bytes {
        let json = serde_json::to_vec(blobs)
            .context("Failed to serialize blobs map for inline storage")?;
        let compressed = to_zstd(&json, ZSTDCompressionLevel::Normal)
            .context("Failed to compress inline blobs")?;
        Ok(Some(compressed))
    } else {
        Ok(None)
    }
}

/// Extract external blob keys from a frame row.
///
/// Returns an empty vec if blobs are inline (nothing to clean up externally)
/// or if the frame has no data (Empty frames).
fn extract_external_blob_keys(row: &FrameRow) -> Result<Vec<String>> {
    let data = match &row.data {
        Some(data) => data,
        None => return Ok(vec![]),
    };

    // Inline blobs are embedded in the row — nothing external to clean up.
    if data.inline_blobs.is_some() {
        return Ok(vec![]);
    }

    let blob_ids = match row.frame_type {
        FrameType::Full => {
            let manifest: ArrayGraphSerializableManifest =
                serde_json::from_str(&data.manifest_json)
                    .context("Failed to parse manifest for blob extraction")?;
            let mut ids = manifest.blobs.get_all_blob_ids();
            ids.push(manifest.self_reference);
            ids
        }
        FrameType::Delta => {
            let manifest: DeltaManifest = serde_json::from_str(&data.manifest_json)
                .context("Failed to parse delta manifest for blob extraction")?;
            let mut ids = manifest.delta_blob;
            ids.push(manifest.self_reference);
            ids
        }
        FrameType::Error => {
            let manifest: ErrorManifest = serde_json::from_str(&data.manifest_json)
                .context("Failed to parse error manifest for blob extraction")?;
            let mut ids = manifest.errors_blob;
            ids.push(manifest.self_reference);
            ids
        }
        FrameType::Empty => return Ok(vec![]),
    };

    Ok(blob_ids.into_iter().map(|id| id.0).collect())
}

/// Fetch a single frame with data via `select_frames`, or error if not found.
async fn get_frame_with_data(
    conn: &mut dyn UnigraphGraphConnection,
    key: &GraphKey,
    task: &ll::Task,
) -> Result<FrameRow> {
    get_frame_with_data_on_conn(conn, key, task)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Frame not found: {:?}", key))
}

/// Fetch a single frame with data via `select_frames`.
/// Returns `None` if the frame does not exist.
async fn get_frame_with_data_on_conn(
    conn: &mut dyn UnigraphGraphConnection,
    key: &GraphKey,
    task: &ll::Task,
) -> Result<Option<FrameRow>> {
    let mut rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                graph_ids: Some(vec![key.graph_id]),
                with_data: Some(true),
                limit: Some(1),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            task,
        )
        .await?;
    Ok(rows.pop())
}

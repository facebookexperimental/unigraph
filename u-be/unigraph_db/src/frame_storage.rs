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

        let manifest_json = row
            .manifest_json
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Frame data missing for {:?}", key))?;

        let manifest: ErrorManifest =
            serde_json::from_str(manifest_json).context("Failed to parse ErrorManifest")?;

        let blobs = self
            .resolve_blobs(&manifest.errors_blob, row.inline_blobs.as_deref(), task)
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
    /// 1. Read the frame's manifest — not its payload; the inline blobs would
    ///    be the frame's whole compressed graph, and inline blobs need no
    ///    cleanup anyway
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
        let row = match get_frame_manifest_on_conn(conn, key, task).await? {
            Some(row) => row,
            None => return Ok(false),
        };

        let blob_keys = external_blob_keys(&row)?;
        if !blob_keys.is_empty() {
            // The other half of the sweep's audit trail: this is where a key
            // is condemned, `delete_swept_blob` is where it dies. Without the
            // frame that scheduled it, a deletion event has no explanation.
            task.data("cleanup_registered", blob_keys.len());
            task.data("cleanup_registered_for", key.to_string());
            conn.register_blobs_for_cleanup(&blob_keys, task).await?;
        }

        conn.delete_frame(key, task).await
    }

    /// Sweep external blobs that have been pending cleanup for at least `min_age`.
    ///
    /// Steps:
    /// 1. Query `blobs_to_delete` for entries older than `now - min_age`
    ///    (newest first, capped at `limit` if set)
    /// 2. Delete each blob from external blob storage
    /// 3. Unregister the blob keys from the cleanup table
    ///
    /// `limit` caps how many blobs a single sweep processes (`None` = no cap),
    /// so a large backlog can be drained incrementally without unbounded work.
    /// Draining newest-first means a persistently-failing old blob can't wedge
    /// the batch and starve cleanup of newly-registered blobs under the cap.
    ///
    /// Returns the number of blobs swept.
    #[ll::task]
    pub async fn sweep_blobs(
        &self,
        min_age: std::time::Duration,
        limit: Option<i64>,
        task: &ll::Task,
    ) -> Result<usize> {
        let now = Timestamp::now().to_unix_timestamp();
        let cutoff = Timestamp::from_unix_timestamp(now - min_age.as_secs() as i64);

        let mut conn = self.graph.conn().await?;
        let blob_keys = conn
            .get_blobs_pending_cleanup_older_than(cutoff, limit, &task)
            .await?;
        drop(conn);

        if blob_keys.is_empty() {
            return Ok(0);
        }
        task.data("candidates", blob_keys.len());

        // Delete from external blob storage in parallel (outside any transaction).
        let delete_futs: Vec<_> = blob_keys
            .iter()
            .map(|key| self.delete_swept_blob(key, &task))
            .collect();
        futures::future::try_join_all(delete_futs).await?;

        // Unregister from cleanup table (separate short-lived connection).
        let mut conn = self.graph.conn_write().await?;
        conn.unregister_blobs_for_cleanup(&blob_keys, &task).await?;

        task.data("swept", blob_keys.len());
        Ok(blob_keys.len())
    }

    /// Delete one swept blob, writing down which key went.
    ///
    /// One event per key, deliberately, because a blob deletion is the one
    /// thing in this system that cannot be undone and the sweep's own tally
    /// only ever says *how many*. When a frame turns up with its blobs missing,
    /// this is the only record of what took them — and until it existed there
    /// was none: nothing on the OSS path logged a key, so the whole question
    /// had to be answered by inference from the sweep's failures.
    ///
    /// Volume is bounded by the sweep's own `limit` (200 on the piggybacked
    /// path), and a sweep with nothing to do emits nothing at all.
    async fn delete_swept_blob(&self, key: &str, task: &ll::Task) -> Result<()> {
        task.spawn("delete_swept_blob", |task| async move {
            task.data("key", key);
            let result = self.blob.delete_blob(key).await;
            if let Err(ref error) = result {
                task.data("delete_error", format!("{error:#}"));
            }
            result
        })
        .await
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

    /// Reconstruct a full graph from a `with_data` frame row.
    #[ll::task]
    pub(crate) async fn reconstruct_full_graph(
        &self,
        row: &FrameRow,
        task: &ll::Task,
    ) -> Result<ArrayGraphSerializable> {
        let manifest_json = require_manifest(row)?;
        let manifest: ArrayGraphSerializableManifest = serde_json::from_str(manifest_json)
            .context("Failed to parse ArrayGraphSerializableManifest")?;

        let all_blob_ids = manifest.blobs.get_all_blob_ids();
        let blobs = self
            .resolve_blobs(&all_blob_ids, row.inline_blobs.as_deref(), &task)
            .await?;

        let manifest_json_bytes = manifest_json.as_bytes().to_vec();

        // CPU-heavy: decompress + deserialize → off the tokio thread
        let task = task.clone();
        tokio::task::spawn_blocking(move || {
            let mut blobs_with_manifest = blobs;
            blobs_with_manifest.insert(manifest.self_reference.clone(), manifest_json_bytes);

            let package = ArrayGraphSerializablePackage {
                manifest,
                blobs: blobs_with_manifest,
            };

            ArrayGraphSerializable::unpack(&package, &task).context("Failed to unpack graph")
        })
        .await
        .context("spawn_blocking panicked")?
    }

    /// Reconstruct a delta from a `with_data` frame row.
    pub(crate) async fn reconstruct_delta(
        &self,
        row: &FrameRow,
        task: &ll::Task,
    ) -> Result<unigraph_core::GraphDelta> {
        let manifest: DeltaManifest = serde_json::from_str(require_manifest(row)?)
            .context("Failed to parse DeltaManifest")?;

        let blobs = self
            .resolve_blobs(&manifest.delta_blob, row.inline_blobs.as_deref(), task)
            .await?;

        let task = task.clone();
        // CPU-heavy: decompress + deserialize → off the tokio thread
        tokio::task::spawn_blocking(move || {
            let package = DeltaPackage { manifest, blobs };
            unpack_delta(&package, &task)
        })
        .await
        .context("spawn_blocking panicked")?
    }

    /// Resolve blobs either from inline data or external blob storage.
    ///
    /// Blob IDs already contain the full storage key (including the
    /// `timeline_id/graph_id/` prefix), so they are used directly as
    /// external blob keys.
    #[ll::task(tags(l3))]
    pub(crate) async fn resolve_blobs(
        &self,
        blob_ids: &[BlobID],
        inline_blobs: Option<&[u8]>,
        _task: &ll::Task,
    ) -> Result<BTreeMap<BlobID, Vec<u8>>> {
        if let Some(compressed) = inline_blobs {
            // Inline: decompress → deserialize (CPU-heavy → off tokio thread)
            let compressed = compressed.to_vec();
            tokio::task::spawn_blocking(move || {
                let decompressed =
                    from_zstd(&compressed).context("Failed to decompress inline blobs")?;
                let all_blobs: BTreeMap<BlobID, Vec<u8>> = serde_json::from_slice(&decompressed)
                    .context("Failed to deserialize inline blobs map")?;
                Ok(all_blobs)
            })
            .await
            .context("spawn_blocking panicked")?
        } else {
            // External: fetch all blobs in parallel
            let futs: Vec<_> = blob_ids
                .iter()
                .map(|blob_id| {
                    let blob_store = &self.blob;
                    let id = blob_id.clone();
                    async move {
                        let data = blob_store.get_blob(&id.0).await?;
                        Ok::<_, anyhow::Error>((id, data))
                    }
                })
                .collect();
            let pairs = futures::future::try_join_all(futs).await?;
            Ok(pairs.into_iter().collect())
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

        // Upload blobs in parallel (outside any transaction).
        let upload_futs: Vec<_> = blobs
            .iter()
            .map(|(blob_id, data)| {
                let blob_store = &self.blob;
                let id = blob_id.0.clone();
                let data = data.clone();
                async move {
                    blob_store
                        .put_blob(&id, &data)
                        .await
                        .with_context(|| format!("Failed to upload blob: {}", id))
                }
            })
            .collect();
        futures::future::try_join_all(upload_futs).await?;

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

/// The manifest of a row that was read with `with_manifest` or `with_data`.
fn require_manifest(row: &FrameRow) -> Result<&str> {
    row.manifest_json.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "{:?} frame graph_id={} has no manifest",
            row.frame_type,
            row.frame.graph_id.0,
        )
    })
}

/// Every external blob key a frame's manifest references.
///
/// Empty when the frame's blobs are inline — those are embedded in the row and
/// disappear with it — or when the frame carries no manifest at all.
///
/// Only meaningful on a row read with `with_manifest` or `with_data`: a
/// metadata-only row knows neither the manifest nor where its blobs live, and
/// reports no keys rather than guessing.
pub(crate) fn external_blob_keys(row: &FrameRow) -> Result<Vec<String>> {
    if row.blobs_are_inline != Some(false) {
        return Ok(vec![]);
    }
    let Some(manifest_json) = row.manifest_json.as_deref() else {
        return Ok(vec![]);
    };

    let blob_ids = match row.frame_type {
        FrameType::Full => {
            let manifest: ArrayGraphSerializableManifest = serde_json::from_str(manifest_json)
                .context("Failed to parse manifest for blob extraction")?;
            let mut ids = manifest.blobs.get_all_blob_ids();
            ids.push(manifest.self_reference);
            ids
        }
        FrameType::Delta => {
            let manifest: DeltaManifest = serde_json::from_str(manifest_json)
                .context("Failed to parse delta manifest for blob extraction")?;
            let mut ids = manifest.delta_blob;
            ids.push(manifest.self_reference);
            ids
        }
        FrameType::Error => {
            let manifest: ErrorManifest = serde_json::from_str(manifest_json)
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
    get_one_frame(conn, key, PayloadRead::Data, task)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Frame not found: {:?}", key))
}

/// Fetch a single frame's manifest, without its inline payload.
/// Returns `None` if the frame does not exist.
async fn get_frame_manifest_on_conn(
    conn: &mut dyn UnigraphGraphConnection,
    key: &GraphKey,
    task: &ll::Task,
) -> Result<Option<FrameRow>> {
    get_one_frame(conn, key, PayloadRead::Manifest, task).await
}

/// How much of a frame's payload [`get_one_frame`] should read.
enum PayloadRead {
    Manifest,
    Data,
}

/// Fetch one frame by key via `select_frames`.
/// Returns `None` if the frame does not exist.
async fn get_one_frame(
    conn: &mut dyn UnigraphGraphConnection,
    key: &GraphKey,
    payload: PayloadRead,
    task: &ll::Task,
) -> Result<Option<FrameRow>> {
    let mut rows = conn
        .select_frames(
            &FrameQuery {
                timeline_id: key.timeline_id.clone(),
                graph_ids: Some(vec![key.graph_id]),
                with_manifest: Some(true),
                with_data: Some(matches!(payload, PayloadRead::Data)),
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

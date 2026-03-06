// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Frame store/fetch/delete operations for [`UnigraphStorage`].
//!
//! Handles packing graphs into manifests + blobs, deciding between inline
//! and external blob storage, transactional frame writes with timeline
//! locking, reconstructing graphs from delta chains, and frame deletion
//! with safe blob cleanup.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;

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
use unigraph_core::apply_delta;
use unigraph_core::derive_delta;
use unigraph_core::pack_delta;
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
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;
use unigraph_storage_core::TimestampedError;
use unigraph_storage_core::UnigraphGraphConnection;

use crate::storage::UnigraphStorage;

impl UnigraphStorage {
    /// Store a full graph snapshot.
    ///
    /// Packs the graph into a manifest + blobs, then stores inline or
    /// externally depending on total blob size.
    pub async fn store_graph_full(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        let config = make_pack_config(key);
        let package = graph.pack(&config).context("Failed to pack graph")?;
        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize graph manifest")?;

        self.store_package(key, FrameType::Full, None, &manifest_json, &package.blobs)
            .await
    }

    /// Store a delta-compressed graph.
    ///
    /// Fetches the base graph, derives the delta, packs it, and stores.
    pub async fn store_graph_delta(
        &self,
        key: &GraphTimeKey,
        base_key: &GraphKey,
        target_graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        let base_graph = self
            .fetch_graph(base_key)
            .await
            .with_context(|| format!("Failed to fetch base graph {:?}", base_key))?;

        let delta = derive_delta(&base_graph, target_graph).context("Failed to derive delta")?;

        let config = make_pack_config(key);
        let package = pack_delta(&delta, &config).context("Failed to pack delta")?;
        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize delta manifest")?;

        self.store_package(
            key,
            FrameType::Delta,
            Some(base_key),
            &manifest_json,
            &package.blobs,
        )
        .await
    }

    /// Store error data for a failed graph computation.
    pub async fn store_error(&self, key: &GraphTimeKey, errors: &[TimestampedError]) -> Result<()> {
        let config = make_pack_config(key);
        let error_count = errors.len() as u32;
        let errors_vec = errors.to_vec();
        let package =
            pack_errors(&errors_vec, error_count, &config).context("Failed to pack errors")?;
        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize error manifest")?;

        self.store_package(key, FrameType::Error, None, &manifest_json, &package.blobs)
            .await
    }

    /// Fetch and reconstruct a graph from storage.
    ///
    /// Handles delta chain resolution: if the frame is a Delta, recursively
    /// fetches the base graph and applies the delta.
    pub async fn fetch_graph(&self, key: &GraphKey) -> Result<ArrayGraphSerializable> {
        let mut visited = HashSet::new();
        self.resolve_graph(key, &mut visited).await
    }

    /// Fetch errors for a frame.
    pub async fn fetch_errors(&self, key: &GraphKey) -> Result<Vec<TimestampedError>> {
        let conn = self.graph.conn().await?;
        let row = get_frame_with_data(&*conn, key).await?;

        if row.frame_type != FrameType::Error {
            anyhow::bail!("Frame {:?} is {:?}, not Error", key, row.frame_type);
        }

        let data = row
            .data
            .ok_or_else(|| anyhow::anyhow!("Frame data missing for {:?}", key))?;

        let manifest: ErrorManifest =
            serde_json::from_str(&data.manifest_json).context("Failed to parse ErrorManifest")?;

        let blobs = self
            .resolve_blobs(&manifest.errors_blob, data.inline_blobs.as_deref())
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
        conn: &dyn UnigraphGraphConnection,
        key: &GraphKey,
    ) -> Result<bool> {
        let row = match get_frame_with_data_on_conn(conn, key).await? {
            Some(row) => row,
            None => return Ok(false),
        };

        let blob_keys = extract_external_blob_keys(&row)?;
        if !blob_keys.is_empty() {
            conn.register_blobs_for_cleanup(&blob_keys).await?;
        }

        conn.delete_frame(key).await
    }

    /// Sweep external blobs that have been pending cleanup for at least `min_age`.
    ///
    /// Steps:
    /// 1. Query `blobs_to_delete` for entries older than `now - min_age`
    /// 2. Delete each blob from external blob storage
    /// 3. Unregister the blob keys from the cleanup table
    ///
    /// Returns the number of blobs swept.
    pub async fn sweep_blobs(&self, min_age: std::time::Duration) -> Result<usize> {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::from_std(min_age)
                .context("Failed to convert min_age to chrono::Duration")?;

        let conn = self.graph.conn().await?;
        let blob_keys = conn.get_blobs_pending_cleanup_older_than(cutoff).await?;
        drop(conn);

        if blob_keys.is_empty() {
            return Ok(0);
        }

        // Delete from external blob storage (outside any transaction).
        for key in &blob_keys {
            self.blob.delete_blob(key).await?;
        }

        // Unregister from cleanup table (separate short-lived connection).
        let conn = self.graph.conn().await?;
        conn.unregister_blobs_for_cleanup(&blob_keys).await?;

        Ok(blob_keys.len())
    }

    /// Compact a timeline by replacing consecutive Full frames with Deltas.
    ///
    /// Walks frames in `(timestamp, graph_id)` order within the given range.
    /// The first Full frame stays Full. Every subsequent Full is replaced with
    /// a Delta derived from the previous data-carrying frame. Empty and Error
    /// frames break the chain (the next Full after them stays Full).
    ///
    /// Returns the number of frames converted from Full to Delta.
    pub async fn compact_timeline(
        &self,
        timeline_id: &TimelineID,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<usize> {
        let conn = self.graph.conn().await?;
        let frames = conn
            .select_frames(&FrameQuery {
                timeline_id: timeline_id.clone(),
                timestamp_bounds: Some(TimestampBounds {
                    start: Some(start),
                    end: Some(end),
                }),
                ..Default::default()
            })
            .await?;
        drop(conn);

        let mut converted = 0;
        let mut prev_data_key: Option<GraphKey> = None;

        for frame in &frames {
            match frame.frame_type {
                FrameType::Full => {
                    if let Some(base_key) = &prev_data_key {
                        self.replace_full_with_delta(timeline_id, base_key, frame)
                            .await?;
                        converted += 1;
                    }
                    prev_data_key = Some(GraphKey {
                        timeline_id: timeline_id.clone(),
                        graph_id: frame.frame.graph_id,
                    });
                }
                FrameType::Delta => {
                    prev_data_key = Some(GraphKey {
                        timeline_id: timeline_id.clone(),
                        graph_id: frame.frame.graph_id,
                    });
                }
                FrameType::Empty | FrameType::Error => {
                    prev_data_key = None;
                }
            }
        }

        Ok(converted)
    }

    /// Replace a Full frame with a Delta derived from a base frame.
    ///
    /// Fetches both graphs, derives the delta, packs it, and atomically
    /// swaps the frame using `delete_frame_on_conn` + `store_package_on_conn`
    /// in a single transaction.
    async fn replace_full_with_delta(
        &self,
        timeline_id: &TimelineID,
        base_key: &GraphKey,
        target_frame: &FrameRow,
    ) -> Result<()> {
        let target_key = GraphKey {
            timeline_id: timeline_id.clone(),
            graph_id: target_frame.frame.graph_id,
        };
        let target_time_key = GraphTimeKey {
            timeline_id: timeline_id.clone(),
            timestamp: target_frame.frame.timestamp,
            graph_id: target_frame.frame.graph_id,
        };

        let base_graph = self
            .fetch_graph(base_key)
            .await
            .with_context(|| format!("Failed to fetch base graph {:?}", base_key))?;
        let target_graph = self
            .fetch_graph(&target_key)
            .await
            .with_context(|| format!("Failed to fetch target graph {:?}", target_key))?;

        let delta = derive_delta(&base_graph, &target_graph).context("Failed to derive delta")?;

        let config = make_pack_config(&target_time_key);
        let package = pack_delta(&delta, &config).context("Failed to pack delta")?;
        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize delta manifest")?;

        // Determine inline vs. external for the new delta.
        let threshold = {
            let conn = self.graph.conn().await?;
            let config = conn
                .get_timeline_config(timeline_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", timeline_id))?;
            config.inline_blob_threshold()
        };

        let inline_blobs = prepare_inline_blobs(&package.blobs, threshold)?;
        let blob_keys_to_unregister = if inline_blobs.is_none() {
            Some(self.upload_blobs(&package.blobs).await?)
        } else {
            None
        };

        // Single transaction: delete old Full + insert new Delta.
        let conn = self.graph.conn().await?;
        conn.start_transaction().await?;
        conn.get_timeline_config_and_lock(timeline_id).await?;

        self.delete_frame_on_conn(&*conn, &target_key).await?;
        self.store_package_on_conn(
            &*conn,
            &target_time_key,
            FrameType::Delta,
            Some(base_key),
            &manifest_json,
            inline_blobs.as_deref(),
            blob_keys_to_unregister.as_deref(),
        )
        .await?;

        conn.commit_transaction().await?;
        Ok(())
    }

    // --- internal helpers ---

    /// Store a packed package (standalone — owns its own transaction).
    ///
    /// Handles the inline-vs-external decision and ensures correct
    /// transactional ordering:
    /// 1. Fetch timeline config to determine inline threshold
    /// 2. If external: register blob keys for cleanup and upload blobs
    ///    (OUTSIDE the db transaction)
    /// 3. Start transaction, lock timeline, write frame
    /// 4. If external: unregister blob keys (INSIDE the transaction)
    /// 5. Commit
    async fn store_package(
        &self,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        blobs: &BTreeMap<BlobID, Vec<u8>>,
    ) -> Result<()> {
        // Read the timeline config (non-locking) to determine inline threshold.
        let threshold = {
            let conn = self.graph.conn().await?;
            let config = conn
                .get_timeline_config(&key.timeline_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Timeline not found: {:?}", key.timeline_id))?;
            config.inline_blob_threshold()
        };

        let inline_blobs = prepare_inline_blobs(blobs, threshold)?;

        // If external, register for cleanup and upload BEFORE the transaction.
        let blob_keys_to_unregister = if inline_blobs.is_none() {
            Some(self.upload_blobs(blobs).await?)
        } else {
            None
        };

        // Start transaction, lock timeline, write frame.
        let conn = self.graph.conn().await?;
        conn.start_transaction().await?;
        conn.get_timeline_config_and_lock(&key.timeline_id)
            .await
            .with_context(|| format!("Failed to lock timeline for {:?}", frame_type))?;

        self.store_package_on_conn(
            &*conn,
            key,
            frame_type,
            base,
            manifest_json,
            inline_blobs.as_deref(),
            blob_keys_to_unregister.as_deref(),
        )
        .await?;

        conn.commit_transaction().await?;
        Ok(())
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
        conn: &dyn UnigraphGraphConnection,
        key: &GraphTimeKey,
        frame_type: FrameType,
        base: Option<&GraphKey>,
        manifest_json: &str,
        inline_blobs: Option<&[u8]>,
        blob_keys_to_unregister: Option<&[String]>,
    ) -> Result<()> {
        conn.store_frame(key, frame_type, base, manifest_json, inline_blobs)
            .await?;

        // Unregister blob keys from cleanup table INSIDE the transaction,
        // so if commit fails the blobs remain registered for cleanup.
        if let Some(blob_keys) = blob_keys_to_unregister {
            conn.unregister_blobs_for_cleanup(blob_keys).await?;
        }

        Ok(())
    }

    /// Recursively resolve a graph, following delta chains.
    async fn resolve_graph(
        &self,
        key: &GraphKey,
        visited: &mut HashSet<GraphKey>,
    ) -> Result<ArrayGraphSerializable> {
        if !visited.insert(key.clone()) {
            anyhow::bail!("Cycle detected in delta chain at {:?}", key);
        }

        let conn = self.graph.conn().await?;
        let row = get_frame_with_data(&*conn, key).await?;
        // Drop the connection before recursing to avoid holding the lock
        drop(conn);

        let data = row
            .data
            .ok_or_else(|| anyhow::anyhow!("Frame data missing for {:?}", key))?;

        match row.frame_type {
            FrameType::Full => self.reconstruct_full_graph(&data).await,
            FrameType::Delta => {
                let base_key = row
                    .base
                    .ok_or_else(|| anyhow::anyhow!("Delta frame {:?} has no base key", key))?;

                let delta = self.reconstruct_delta(&data).await?;
                let base_graph = Box::pin(self.resolve_graph(&base_key, visited)).await?;
                apply_delta(&base_graph, &delta).context("Failed to apply delta")
            }
            FrameType::Error => {
                anyhow::bail!("Cannot fetch graph for error frame {:?}", key);
            }
            FrameType::Empty => {
                anyhow::bail!("Cannot fetch graph for empty frame {:?}", key);
            }
        }
    }

    /// Reconstruct a full graph from frame data.
    async fn reconstruct_full_graph(&self, data: &FrameData) -> Result<ArrayGraphSerializable> {
        let manifest: ArrayGraphSerializableManifest = serde_json::from_str(&data.manifest_json)
            .context("Failed to parse ArrayGraphSerializableManifest")?;

        let all_blob_ids = manifest.blobs.get_all_blob_ids();
        let blobs = self
            .resolve_blobs(&all_blob_ids, data.inline_blobs.as_deref())
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
    async fn reconstruct_delta(&self, data: &FrameData) -> Result<unigraph_core::GraphDelta> {
        let manifest: DeltaManifest =
            serde_json::from_str(&data.manifest_json).context("Failed to parse DeltaManifest")?;

        let blobs = self
            .resolve_blobs(&manifest.delta_blob, data.inline_blobs.as_deref())
            .await?;

        let package = DeltaPackage { manifest, blobs };
        unpack_delta(&package).context("Failed to unpack delta")
    }

    /// Resolve blobs either from inline data or external blob storage.
    ///
    /// Blob IDs already contain the full storage key (including the
    /// `timeline_id/graph_id/` prefix), so they are used directly as
    /// external blob keys.
    async fn resolve_blobs(
        &self,
        blob_ids: &[BlobID],
        inline_blobs: Option<&[u8]>,
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
    pub async fn upload_blobs(&self, blobs: &BTreeMap<BlobID, Vec<u8>>) -> Result<Vec<String>> {
        let blob_keys: Vec<String> = blobs.keys().map(|id| id.0.clone()).collect();

        // Register for cleanup using a separate short-lived connection.
        let reg_conn = self.graph.conn().await?;
        reg_conn.register_blobs_for_cleanup(&blob_keys).await?;
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
}

/// Build a pack config that prefixes all blob IDs with `timeline_id/graph_id/`.
///
/// This ensures each frame's blobs have unique IDs and can be independently
/// deleted when the frame is removed.
fn make_pack_config(key: &GraphTimeKey) -> ArrayGraphSerializablePackageConfig {
    let timeline_id = key.timeline_id.clone();
    let graph_id = key.graph_id;
    ArrayGraphSerializablePackageConfig {
        modify_blob_id: Some(Arc::new(move |id| {
            BlobID(format!("{}/{}/{}", timeline_id.0, graph_id.0, id))
        })),
        ..Default::default()
    }
}

/// Check total blob size and return compressed inline bytes if under the
/// threshold, or `None` if blobs should be stored externally.
///
/// Use `TimelineConfig::inline_blob_threshold()` to get the threshold for
/// a specific timeline, or `DEFAULT_INLINE_BLOB_THRESHOLD_BYTES` for the default.
pub fn prepare_inline_blobs(
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
    conn: &dyn UnigraphGraphConnection,
    key: &GraphKey,
) -> Result<FrameRow> {
    get_frame_with_data_on_conn(conn, key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Frame not found: {:?}", key))
}

/// Fetch a single frame with data via `select_frames`.
/// Returns `None` if the frame does not exist.
async fn get_frame_with_data_on_conn(
    conn: &dyn UnigraphGraphConnection,
    key: &GraphKey,
) -> Result<Option<FrameRow>> {
    let mut rows = conn
        .select_frames(&FrameQuery {
            timeline_id: key.timeline_id.clone(),
            graph_ids: Some(vec![key.graph_id]),
            with_data: Some(true),
            limit: Some(1),
            ..Default::default()
        })
        .await?;
    Ok(rows.pop())
}

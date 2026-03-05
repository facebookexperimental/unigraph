// Copyright (c) Meta Platforms, Inc. and affiliates.

//! The [`UnigraphStorage`] compositor — high-level graph store/fetch operations.
//!
//! Combines a [`UnigraphGraphStorage`] (frame metadata + inline blobs) with a
//! [`UnigraphBlobStorage`] (external blob storage) to provide full graph
//! lifecycle management: store full graphs, store deltas, store errors,
//! and reconstruct graphs from delta chains.

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

use crate::frame::FrameData;
use crate::traits::UnigraphBlobStorage;
use crate::traits::UnigraphGraphStorage;
use crate::types::FrameType;
use crate::types::GraphID;
use crate::types::GraphKey;
use crate::types::GraphTimeKey;
use crate::types::INLINE_BLOB_THRESHOLD_BYTES;
use crate::types::TimelineID;
use crate::types::TimestampedError;

/// High-level storage compositor that provides graph store/fetch operations.
///
/// Delegates low-level persistence to a [`UnigraphGraphStorage`] (frames table)
/// and a [`UnigraphBlobStorage`] (external blob store). Handles the decision
/// of whether to inline blobs or store them externally based on
/// [`INLINE_BLOB_THRESHOLD_BYTES`].
pub struct UnigraphStorage {
    pub graph: Arc<dyn UnigraphGraphStorage>,
    pub blob: Arc<dyn UnigraphBlobStorage>,
}

impl UnigraphStorage {
    pub fn new(graph: Arc<dyn UnigraphGraphStorage>, blob: Arc<dyn UnigraphBlobStorage>) -> Self {
        Self { graph, blob }
    }

    /// Store a full graph snapshot.
    ///
    /// Packs the graph into a manifest + blobs, then stores inline or
    /// externally depending on total blob size.
    pub fn store_graph_full(
        &self,
        key: &GraphTimeKey,
        graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        let config = ArrayGraphSerializablePackageConfig::default();
        let package = graph.pack(&config).context("Failed to pack graph")?;

        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize graph manifest")?;

        let inline_blobs =
            self.store_blobs_if_needed(&package.blobs, &key.timeline_id, &key.graph_id)?;

        self.graph.store_frame(
            key,
            FrameType::Full,
            None,
            &manifest_json,
            inline_blobs.as_deref(),
        )
    }

    /// Store a delta-compressed graph.
    ///
    /// Fetches the base graph, derives the delta, packs it, and stores.
    pub fn store_graph_delta(
        &self,
        key: &GraphTimeKey,
        base_key: &GraphKey,
        target_graph: &ArrayGraphSerializable,
    ) -> Result<()> {
        let base_graph = self
            .fetch_graph(base_key)
            .with_context(|| format!("Failed to fetch base graph {:?}", base_key))?;

        let delta = derive_delta(&base_graph, target_graph).context("Failed to derive delta")?;

        let config = ArrayGraphSerializablePackageConfig::default();
        let package = pack_delta(&delta, &config).context("Failed to pack delta")?;

        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize delta manifest")?;

        let inline_blobs =
            self.store_blobs_if_needed(&package.blobs, &key.timeline_id, &key.graph_id)?;

        self.graph.store_frame(
            key,
            FrameType::Delta,
            Some(base_key),
            &manifest_json,
            inline_blobs.as_deref(),
        )
    }

    /// Store error data for a failed graph computation.
    pub fn store_error(&self, key: &GraphTimeKey, errors: &[TimestampedError]) -> Result<()> {
        let config = ArrayGraphSerializablePackageConfig::default();
        let error_count = errors.len() as u32;
        let errors_vec = errors.to_vec();
        let package =
            pack_errors(&errors_vec, error_count, &config).context("Failed to pack errors")?;

        let manifest_json = serde_json::to_string(&package.manifest)
            .context("Failed to serialize error manifest")?;

        let inline_blobs =
            self.store_blobs_if_needed(&package.blobs, &key.timeline_id, &key.graph_id)?;

        self.graph.store_frame(
            key,
            FrameType::Error,
            None,
            &manifest_json,
            inline_blobs.as_deref(),
        )
    }

    /// Fetch and reconstruct a graph from storage.
    ///
    /// Handles delta chain resolution: if the frame is a Delta, recursively
    /// fetches the base graph and applies the delta.
    pub fn fetch_graph(&self, key: &GraphKey) -> Result<ArrayGraphSerializable> {
        let mut visited = HashSet::new();
        self.resolve_graph(key, &mut visited)
    }

    /// Fetch errors for a frame.
    pub fn fetch_errors(&self, key: &GraphKey) -> Result<Vec<TimestampedError>> {
        let row = self
            .graph
            .get_frame(key, true)?
            .ok_or_else(|| anyhow::anyhow!("Frame not found: {:?}", key))?;

        if row.frame_type != FrameType::Error {
            anyhow::bail!("Frame {:?} is {:?}, not Error", key, row.frame_type);
        }

        let data = row
            .data
            .ok_or_else(|| anyhow::anyhow!("Frame data missing for {:?}", key))?;

        let manifest: ErrorManifest =
            serde_json::from_str(&data.manifest_json).context("Failed to parse ErrorManifest")?;

        let blobs = self.resolve_blobs(
            &manifest.errors_blob,
            data.inline_blobs.as_deref(),
            &key.timeline_id,
            &key.graph_id,
        )?;

        let package = ErrorPackage { manifest, blobs };
        unpack_errors(&package).context("Failed to unpack errors")
    }

    // --- internal helpers ---

    /// Recursively resolve a graph, following delta chains.
    fn resolve_graph(
        &self,
        key: &GraphKey,
        visited: &mut HashSet<GraphKey>,
    ) -> Result<ArrayGraphSerializable> {
        if !visited.insert(key.clone()) {
            anyhow::bail!("Cycle detected in delta chain at {:?}", key);
        }

        let row = self
            .graph
            .get_frame(key, true)?
            .ok_or_else(|| anyhow::anyhow!("Frame not found: {:?}", key))?;

        let data = row
            .data
            .ok_or_else(|| anyhow::anyhow!("Frame data missing for {:?}", key))?;

        match row.frame_type {
            FrameType::Full => self.reconstruct_full_graph(&data, &key.timeline_id, &key.graph_id),
            FrameType::Delta => {
                let base_key = row
                    .base
                    .ok_or_else(|| anyhow::anyhow!("Delta frame {:?} has no base key", key))?;

                let delta = self.reconstruct_delta(&data, &key.timeline_id, &key.graph_id)?;
                let base_graph = self.resolve_graph(&base_key, visited)?;
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
    fn reconstruct_full_graph(
        &self,
        data: &FrameData,
        timeline_id: &TimelineID,
        graph_id: &GraphID,
    ) -> Result<ArrayGraphSerializable> {
        let manifest: ArrayGraphSerializableManifest = serde_json::from_str(&data.manifest_json)
            .context("Failed to parse ArrayGraphSerializableManifest")?;

        let all_blob_ids = manifest.blobs.get_all_blob_ids();
        let blobs = self.resolve_blobs(
            &all_blob_ids,
            data.inline_blobs.as_deref(),
            timeline_id,
            graph_id,
        )?;

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
    fn reconstruct_delta(
        &self,
        data: &FrameData,
        timeline_id: &TimelineID,
        graph_id: &GraphID,
    ) -> Result<unigraph_core::GraphDelta> {
        let manifest: DeltaManifest =
            serde_json::from_str(&data.manifest_json).context("Failed to parse DeltaManifest")?;

        let blobs = self.resolve_blobs(
            &manifest.delta_blob,
            data.inline_blobs.as_deref(),
            timeline_id,
            graph_id,
        )?;

        let package = DeltaPackage { manifest, blobs };
        unpack_delta(&package).context("Failed to unpack delta")
    }

    /// Resolve blobs either from inline data or external blob storage.
    fn resolve_blobs(
        &self,
        blob_ids: &[BlobID],
        inline_blobs: Option<&[u8]>,
        timeline_id: &TimelineID,
        graph_id: &GraphID,
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
                let key = format!("{}/{}/{}", timeline_id.0, graph_id.0, blob_id.0);
                let data = self
                    .blob
                    .get_blob(&key)?
                    .ok_or_else(|| anyhow::anyhow!("Missing external blob: {}", key))?;
                result.insert(blob_id.clone(), data);
            }
            Ok(result)
        }
    }

    /// Decide whether to inline blobs or store externally, and do the storage.
    ///
    /// Returns `Some(compressed_bytes)` if blobs are inlined, `None` if stored externally.
    fn store_blobs_if_needed(
        &self,
        blobs: &BTreeMap<BlobID, Vec<u8>>,
        timeline_id: &TimelineID,
        graph_id: &GraphID,
    ) -> Result<Option<Vec<u8>>> {
        let total_size_bytes: usize = blobs.values().map(|b| b.len()).sum();

        if total_size_bytes <= INLINE_BLOB_THRESHOLD_BYTES {
            // Inline: serialize blob map → ZSTD compress
            let json = serde_json::to_vec(blobs)
                .context("Failed to serialize blobs map for inline storage")?;
            let compressed = to_zstd(&json, ZSTDCompressionLevel::Normal)
                .context("Failed to compress inline blobs")?;
            Ok(Some(compressed))
        } else {
            // External: upload each blob
            let prefix = format!("{}/{}", timeline_id.0, graph_id.0);
            let blob_keys: Vec<String> = blobs
                .keys()
                .map(|id| format!("{}/{}", prefix, id.0))
                .collect();

            // Register for cleanup in case we fail partway through
            self.graph.register_blobs_for_cleanup(&blob_keys)?;

            for (blob_id, data) in blobs {
                let key = format!("{}/{}", prefix, blob_id.0);
                self.blob
                    .put_blob(&key, data)
                    .with_context(|| format!("Failed to upload blob: {}", key))?;
            }

            // Success — unregister from cleanup
            self.graph.unregister_blobs_for_cleanup(&blob_keys)?;

            Ok(None)
        }
    }
}

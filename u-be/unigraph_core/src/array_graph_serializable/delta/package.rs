// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Chunked, compressed blob packaging for [`GraphDelta`].
//!
//! Mirrors the blob packaging approach used by [`super::super::package`] for
//! full graphs, but applied to deltas. The entire [`GraphDelta`] is serialized
//! as a single logical field, then compressed and chunked into content-addressed
//! blobs.
//!
//! ## Pack / Unpack
//!
//! - [`pack_delta`] — serialize a delta into blobs + manifest.
//! - [`unpack_delta`] — reconstruct a delta from blobs + manifest.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

use super::GraphDelta;
use crate::array_graph_serializable::package::ArrayGraphSerializablePackageConfig;
use crate::array_graph_serializable::package::BlobID;
use crate::array_graph_serializable::package::into_blobs;

/// Manifest for a packaged delta.
///
/// Stores metadata about the delta (base graph reference, statistics) and
/// references to the blob(s) containing the serialized [`GraphDelta`].
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DeltaManifest {
    /// Blob ID that points to the JSON-serialized manifest itself.
    pub self_reference: BlobID,
    /// Statistics about the delta contents.
    pub stats: DeltaManifestStats,
    /// Blob IDs containing the serialized [`GraphDelta`] (chunked if large).
    pub delta_blob: Vec<BlobID>,
}

/// Statistics about a packaged delta.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DeltaManifestStats {
    /// Total number of data blobs (excludes the manifest blob itself).
    pub total_blobs: u32,
    /// Sum of all blob sizes in bytes (compressed).
    pub total_size_bytes: u32,
    /// Per-blob compressed size map.
    pub blob_sizes_bytes: BTreeMap<BlobID, u32>,
    /// Number of nodes added in this delta.
    pub nodes_added: u32,
    /// Number of nodes removed in this delta.
    pub nodes_removed: u32,
    /// Number of nodes with any edge, metric, label, or property change.
    pub nodes_changed: u32,
    /// Number of directed edges added across all nodes.
    pub directed_edges_added: u32,
    /// Number of directed edges removed across all nodes.
    pub directed_edges_removed: u32,
    /// Number of source nodes with tagged edge changes.
    pub tagged_edges_changed: u32,
    /// Number of source nodes with dynamic edge replacements.
    pub dynamic_edges_changed: u32,
    /// Number of (metric_name, node) pairs changed.
    pub metrics_changed: u32,
    /// Number of nodes with label changes.
    pub labels_changed: u32,
    /// Number of nodes with property changes.
    pub properties_changed: u32,
    /// Whether graph-level properties changed.
    pub graph_properties_changed: bool,
}

/// A fully self-contained delta package: the manifest plus all blob data.
pub struct DeltaPackage {
    pub manifest: DeltaManifest,
    pub blobs: BTreeMap<BlobID, Vec<u8>>,
}

impl DeltaManifestStats {
    /// Compute statistics from a [`GraphDelta`] and the resulting blobs.
    fn from_delta_and_blobs(delta: &GraphDelta, blobs: &BTreeMap<BlobID, Vec<u8>>) -> Self {
        let total_blobs = blobs.len() as u32;
        let total_size_bytes = blobs.values().map(|b| b.len()).sum::<usize>() as u32;
        let blob_sizes_bytes = blobs
            .iter()
            .map(|(k, v)| (k.clone(), v.len() as u32))
            .collect();

        let empty_nodes = unigraph_delta::MapDelta {
            added: BTreeMap::new(),
            removed: std::collections::BTreeSet::new(),
            changed: BTreeMap::new(),
        };
        let nodes = delta.nodes.as_ref().unwrap_or(&empty_nodes);

        let mut directed_edges_added: u32 = 0;
        let mut directed_edges_removed: u32 = 0;
        let mut tagged_edges_changed: u32 = 0;
        let mut dynamic_edges_changed: u32 = 0;
        let mut metrics_changed: u32 = 0;
        let mut labels_changed: u32 = 0;
        let mut properties_changed: u32 = 0;

        for node_delta in nodes.changed.values() {
            if let Some(ref dir) = node_delta.edges_directed {
                if let unigraph_delta::OptionDelta::Changed(set_delta) = dir {
                    directed_edges_added += set_delta.added.len() as u32;
                    directed_edges_removed += set_delta.removed.len() as u32;
                }
            }
            if node_delta.edges_tagged.is_some() {
                tagged_edges_changed += 1;
            }
            if node_delta.edges_dynamic.is_some() {
                dynamic_edges_changed += 1;
            }
            if node_delta.metrics.is_some() {
                metrics_changed += 1;
            }
            if node_delta.labels.is_some() {
                labels_changed += 1;
            }
            if node_delta.properties.is_some() {
                properties_changed += 1;
            }
        }

        DeltaManifestStats {
            total_blobs,
            total_size_bytes,
            blob_sizes_bytes,
            nodes_added: nodes.added.len() as u32,
            nodes_removed: nodes.removed.len() as u32,
            nodes_changed: nodes.changed.len() as u32,
            directed_edges_added,
            directed_edges_removed,
            tagged_edges_changed,
            dynamic_edges_changed,
            metrics_changed,
            labels_changed,
            properties_changed,
            graph_properties_changed: delta.properties.is_some(),
        }
    }
}

/// Pack a [`GraphDelta`] into a manifest + blobs.
///
/// The delta is serialized as JSON, ZSTD-compressed, and split into
/// content-addressed chunks using the same infrastructure as graph packaging.
pub fn pack_delta(
    delta: &GraphDelta,
    config: &ArrayGraphSerializablePackageConfig,
) -> Result<DeltaPackage> {
    let mut blobs = BTreeMap::new();

    let delta_blob = into_blobs(delta, "delta", &mut blobs, config)?;

    let mut manifest_blob_id = BlobID::from("_delta_manifest.json");
    if let Some(f) = config.modify_blob_id.as_ref() {
        manifest_blob_id = f(&manifest_blob_id.0);
    }

    let stats = DeltaManifestStats::from_delta_and_blobs(delta, &blobs);

    let manifest = DeltaManifest {
        self_reference: manifest_blob_id.clone(),
        stats,
        delta_blob,
    };

    blobs.insert(
        manifest_blob_id,
        serde_json::to_string_pretty(&manifest)?.into_bytes(),
    );

    Ok(DeltaPackage { manifest, blobs })
}

/// Unpack a [`DeltaPackage`] back into a [`GraphDelta`].
pub fn unpack_delta(package: &DeltaPackage) -> Result<GraphDelta> {
    use unigraph_serialization::from_zstd;

    (|| {
        let mut combined_data = Vec::new();
        for blob_id in &package.manifest.delta_blob {
            let chunk = package
                .blobs
                .get(blob_id)
                .ok_or_else(|| anyhow::anyhow!("Missing blob: {}", blob_id.0))?;
            combined_data.extend_from_slice(chunk);
        }

        let json = from_zstd(&combined_data)?;
        let delta: GraphDelta =
            serde_json::from_slice(&json).context("Failed to deserialize GraphDelta JSON")?;
        anyhow::Ok(delta)
    })()
    .context("Failed to unpack delta")
    .with_context(|| {
        format!(
            "DeltaManifest: {}",
            serde_json::to_string_pretty(&package.manifest).unwrap_or_default()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive_delta;
    use crate::tests::test_graphs::make_test_array_graph_2;

    #[test]
    fn roundtrip_delta_pack_unpack() -> Result<()> {
        let base = make_test_array_graph_2()?.into_serializable();
        // Create a slightly different graph by re-serializing
        // For a meaningful delta, we'll just use the same graph (empty delta)
        let target = make_test_array_graph_2()?.into_serializable();

        let delta = derive_delta(&base, &target)?;

        let package = pack_delta(&delta, &ArrayGraphSerializablePackageConfig::default())?;
        let roundtripped = unpack_delta(&package)?;

        let original_json = serde_json::to_string_pretty(&delta)?;
        let roundtripped_json = serde_json::to_string_pretty(&roundtripped)?;
        assert_eq!(original_json, roundtripped_json);

        Ok(())
    }
}

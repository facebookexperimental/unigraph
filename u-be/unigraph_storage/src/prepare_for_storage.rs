// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphNodes;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializableEdges;
use unigraph_core::ArrayGraphSerializableNodeMetadata;
use xxhash_rust::xxh3::xxh3_64;

use crate::manifest::ArrayGraphSerializableManifest;
use crate::manifest::BlobID;
use crate::manifest::ManifestBlobs;

const DEFAULT_BYTES_PER_BLOB_CHUNK: usize = 2_000_000; // 2 MB
const DEFAULT_COMPRESSION_LEVEL: i32 = 8; // ZSTD compression level

type ModifyBlobID = Option<Arc<dyn Fn(&str) -> BlobID + Send + Sync>>;

#[derive(Default, Clone)]
pub struct StorageConfig {
    pub bytes_per_blob_chunk: Option<usize>,

    /// ZSTD compression level
    pub compression_level: Option<i32>,

    /// A function that generates an ID for the blob.
    /// This is here to decouple the logic of preparing the graph
    /// for storage from the blobstorage implementation.
    /// e.g. in Manifold we're likely gonna store graphs
    /// under a certain namespace and a GraphID folder, which
    /// should not be a concern of the logic in this file.
    pub modify_blob_id: ModifyBlobID,
}

impl StorageConfig {
    pub fn bytes_per_chunk(&self) -> usize {
        self.bytes_per_blob_chunk
            .unwrap_or(DEFAULT_BYTES_PER_BLOB_CHUNK)
    }

    pub fn compression_level(&self) -> i32 {
        self.compression_level.unwrap_or(DEFAULT_COMPRESSION_LEVEL)
    }
}

/// Converts an `ArrayGraphSerializable` into a manifest and a collection of blobs
/// that can be stored separately and later reconstructed using `from_blobs`.
///
/// # Arguments
/// * `graph` - The graph to serialize
/// * `c` - Storage configuration including chunking and compression settings
///
/// # Returns
/// A tuple of (manifest, blobs) where:
/// - manifest contains the metadata and blob IDs
/// - blobs is a map from BlobID to the actual blob data
pub fn to_blobs(
    graph: &ArrayGraphSerializable,
    c: &StorageConfig,
) -> Result<(ArrayGraphSerializableManifest, BTreeMap<BlobID, Vec<u8>>)> {
    let mut b = BTreeMap::new();

    let ArrayGraphSerializable {
        node_names_ordered,
        edges,
        node_metadata,
        graph_settings,
        traversal_config,
        entry_points,
    } = &graph;

    let ArrayGraphSerializableEdges {
        directed,
        directed_offsets,
        tagged,
        dynamic,
    } = &edges;

    let ArrayGraphSerializableNodeMetadata { metrics, tag_sets } = &node_metadata;

    let (node_names, node_names_offsets) = node_names_ordered.as_parts();
    let node_names = into_blobs(node_names, "node_names", &mut b, c)?;
    let node_names_offsets = into_blobs(node_names_offsets, "node_names_offsets", &mut b, c)?;
    let directed = into_blobs(directed, "directed", &mut b, c)?;
    let directed_offsets = into_blobs(directed_offsets, "directed_offsets", &mut b, c)?;
    let tagged = into_blobs(tagged, "tagged", &mut b, c)?;
    let dynamic = into_blobs(dynamic, "dynamic", &mut b, c)?;
    let metrics = into_blobs(metrics, "metrics", &mut b, c)?;
    let tag_sets = into_blobs(tag_sets, "tag_sets", &mut b, c)?;
    let traversal_config = into_blobs(traversal_config, "traversal_config", &mut b, c)?;
    let entry_points = into_blobs(entry_points, "entry_points", &mut b, c)?;

    let manifest_blobs = ManifestBlobs {
        node_names,
        node_names_offsets,
        directed,
        directed_offsets,
        tagged,
        dynamic,
        metrics,
        tag_sets,
        traversal_config,
        entry_points,
    };

    Ok((
        ArrayGraphSerializableManifest {
            blobs: manifest_blobs,
            graph_settings: graph_settings.clone(),
        },
        b,
    ))
}

/// Restores an `ArrayGraphSerializable` from a manifest and blob data.
/// This is the exact inverse of the `to_blobs` function.
///
/// # Arguments
/// * `manifest` - The manifest containing metadata and blob IDs
/// * `blobs` - Map from BlobID to the actual blob data. This would be fetched from
///   the underlying storage where the graph was stored (db/filesystem/etc)
pub fn from_blobs(
    manifest: &ArrayGraphSerializableManifest,
    b: &BTreeMap<BlobID, Vec<u8>>,
) -> Result<ArrayGraphSerializable> {
    let &ArrayGraphSerializableManifest {
        blobs,
        graph_settings,
    } = &manifest;
    let ManifestBlobs {
        node_names,
        node_names_offsets, // This is the same data as node_names, so we ignore it
        directed,
        directed_offsets,
        tagged,
        dynamic,
        metrics,
        tag_sets,
        traversal_config,
        entry_points,
    } = &blobs;

    // Reconstruct each field by combining chunks and deserializing
    let node_names = from_blobs_field(node_names, b)?;
    let node_name_offsets = from_blobs_field(node_names_offsets, b)?;

    let directed = from_blobs_field(directed, b)?;
    let directed_offsets = from_blobs_field(directed_offsets, b)?;
    let tagged = from_blobs_field(tagged, b)?;
    let dynamic = from_blobs_field(dynamic, b)?;
    let metrics = from_blobs_field(metrics, b)?;
    let tag_sets = from_blobs_field(tag_sets, b)?;
    let traversal_config = from_blobs_field(traversal_config, b)?;
    let entry_points = from_blobs_field(entry_points, b)?;

    let edges = ArrayGraphSerializableEdges {
        directed,
        directed_offsets,
        tagged,
        dynamic,
    };

    let node_metadata = ArrayGraphSerializableNodeMetadata { metrics, tag_sets };

    Ok(ArrayGraphSerializable {
        node_names_ordered: Arc::new(ArrayGraphNodes::from_parts(node_names, node_name_offsets)),
        edges,
        node_metadata,
        graph_settings: graph_settings.clone(),
        traversal_config,
        entry_points,
    })
}

fn from_blobs_field<T: serde::de::DeserializeOwned + Default>(
    blob_ids: &[BlobID],
    all_blobs: &BTreeMap<BlobID, Vec<u8>>,
) -> Result<T> {
    (|| {
        // Reconstruct the original data by combining chunks in order
        let mut combined_data = Vec::new();

        for blob_id in blob_ids {
            let chunk = all_blobs
                .get(blob_id)
                .ok_or_else(|| anyhow::anyhow!("Missing blob: {}", blob_id.0))?;
            combined_data.extend_from_slice(chunk);
        }

        // Decompress the combined data
        let json = zstd::bulk::decompress(&combined_data, DEFAULT_BYTES_PER_BLOB_CHUNK * 10)
            .context("zstd decompression failed")?;

        // Deserialize from JSON
        let value: T = serde_json::from_slice(&json).context("Failed to deserialize JSON")?;
        anyhow::Ok(value)
    })()
    .with_context(|| {
        format!(
            "Failed to deserialize field: {:?}. BlobIDs: {:?}",
            std::any::type_name::<T>(),
            &blob_ids
        )
    })
}

fn into_blobs<T: serde::Serialize>(
    value: &T,
    name: &str,
    all_blobs: &mut BTreeMap<BlobID, Vec<u8>>,
    cfg: &StorageConfig,
) -> Result<Vec<BlobID>> {
    let json = serde_json::to_vec(value)?;
    let zstd = zstd::bulk::compress(&json, cfg.compression_level())?;

    let result: BTreeMap<BlobID, Vec<u8>> = into_chunks(zstd, cfg.bytes_per_chunk())
        .into_iter()
        .map(|chunk| {
            let xx = xxh3_64(&chunk);
            let blob_id = BlobID(format!("{name}_{xx}"));
            (blob_id, chunk)
        })
        .collect();

    let ids = result.keys().cloned().collect();
    all_blobs.extend(result);
    Ok(ids)
}

fn into_chunks(blob: Vec<u8>, chunk_size_bytes: usize) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut remaining = blob;

    while remaining.len() > chunk_size_bytes {
        let remainder = remaining.split_off(chunk_size_bytes);
        chunks.push(remaining);
        remaining = remainder;
    }

    chunks.push(remaining);
    chunks
}

#[cfg(test)]
mod tests {
    use k9::snapshot;
    use unigraph_core::MapGraph;

    use super::*;

    const TEST_GRAPH_2: &str =
        include_str!("../../unigraph_core/src/tests/test_graphs/test_graph_2.json");

    fn make_graph() -> ArrayGraphSerializable {
        MapGraph::from_json(TEST_GRAPH_2)
            .unwrap()
            .to_array_graph_serializable()
            .unwrap()
    }

    #[test]
    fn serialize() -> Result<()> {
        let g = make_graph();

        let (manifest, blobs) = to_blobs(
            &g,
            &StorageConfig {
                bytes_per_blob_chunk: Some(50),
                compression_level: Some(17),
                modify_blob_id: Some(Arc::new(|id| BlobID(id.to_string()))),
            },
        )?;

        snapshot!(
            serde_json::to_string_pretty(&manifest)?,
            r#"
{
  "blobs": {
    "node_names": [
      "node_names_6281809031306549709"
    ],
    "node_names_offsets": [
      "node_names_offsets_6491002063904830174"
    ],
    "directed": [
      "directed_3098825159700367953"
    ],
    "directed_offsets": [
      "directed_offsets_17048802696332253084"
    ],
    "tagged": [
      "tagged_10664201214824955125",
      "tagged_8048188434168318281"
    ],
    "dynamic": [
      "dynamic_17709666951863227118",
      "dynamic_3675328647461951329"
    ],
    "metrics": [
      "metrics_17201045065729657183"
    ],
    "tag_sets": [
      "tag_sets_121953578755559923",
      "tag_sets_16961032930212497945"
    ],
    "traversal_config": [
      "traversal_config_12265251058727778867"
    ],
    "entry_points": [
      "entry_points_12265251058727778867"
    ]
  },
  "graph_settings": null
}
"#
        );

        snapshot!(
            blobs.keys().cloned().map(|id| id.0).collect::<Vec<_>>(),
            r#"
[
    "directed_3098825159700367953",
    "directed_offsets_17048802696332253084",
    "dynamic_17709666951863227118",
    "dynamic_3675328647461951329",
    "entry_points_12265251058727778867",
    "metrics_17201045065729657183",
    "node_names_6281809031306549709",
    "node_names_offsets_6491002063904830174",
    "tag_sets_121953578755559923",
    "tag_sets_16961032930212497945",
    "tagged_10664201214824955125",
    "tagged_8048188434168318281",
    "traversal_config_12265251058727778867",
]
"#
        );
        Ok(())
    }

    #[test]
    fn roundtrip_to_blobs_and_from_blobs() -> Result<()> {
        let original_graph = make_graph();

        // Convert to blobs
        let (manifest, blobs) = to_blobs(&original_graph, &StorageConfig::default())?;

        // Convert back from blobs
        let reconstructed_graph = from_blobs(&manifest, &blobs)?;

        // Verify they're the same (by comparing JSON representations)
        let original_json = serde_json::to_string_pretty(&original_graph)?;
        let reconstructed_json = serde_json::to_string_pretty(&reconstructed_graph)?;

        assert_eq!(original_json, reconstructed_json);
        Ok(())
    }
}

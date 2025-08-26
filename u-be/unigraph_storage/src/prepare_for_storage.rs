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
use crate::manifest::ArrayGraphSerializablePackage;
use crate::manifest::BlobID;
use crate::manifest::ManifestBlobs;
use crate::manifest::ManifestStats;

/// Macro to serialize multiple fields to blobs in parallel using Rayon.
///
/// Usage: `into_blobs_parallelized!(field1, field2, field3; all_blobs, config)`
/// Returns a tuple of Vec<BlobID> in the same order.
macro_rules! into_blobs_parallelized {
    ($($field:ident),* ; $all_blobs:expr, $config:expr) => {
        {
            use rayon::scope;
            use std::sync::Mutex;
            use paste::paste;

            // Wrap the all_blobs map in a mutex
            let all_blobs_mutex = Mutex::new($all_blobs);

            paste! {
                // Create individual result variables for each field
                $(
                    let mut [<result_ $field>] = None;
                )*
            }

            scope(|s| {
                $(
                    s.spawn(|_| {
                        let mut temp_blobs = std::collections::BTreeMap::new();
                        let result = into_blobs(&$field, stringify!($field), &mut temp_blobs, $config)
                            .with_context(|| format!("Failed to serialize field {}", stringify!($field)));

                        paste! {
                            [<result_ $field>] = Some(result);
                        }
                        // Merge temp_blobs into the shared all_blobs map
                        let mut all_blobs_guard = all_blobs_mutex.lock().unwrap();
                        all_blobs_guard.extend(temp_blobs);
                    });
                )*
            });

            // Return tuple with all results in order
            paste! {
                ($(
                    [<result_ $field>].context("Empty value")??,
                )*)
            }
        }
    };
}

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
pub fn pack(
    graph: &ArrayGraphSerializable,
    c: &StorageConfig,
) -> Result<ArrayGraphSerializablePackage> {
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

    let (
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
    ) = into_blobs_parallelized!(
        node_names,
        node_names_offsets,
        directed,
        directed_offsets,
        tagged,
        dynamic,
        metrics,
        tag_sets,
        traversal_config,
        entry_points;
        &mut b, c
    );

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

    let mut manifest_blob_id = BlobID::from("_manifest.json");

    if let Some(f) = c.modify_blob_id.as_ref() {
        manifest_blob_id = f(&manifest_blob_id.0);
    }

    let stats = ManifestStats::from_blobs(&b, graph);

    let manifest = ArrayGraphSerializableManifest {
        self_reference: manifest_blob_id.clone(),
        stats,
        blobs: manifest_blobs,
        graph_settings: graph_settings.clone(),
    };

    b.insert(
        manifest_blob_id,
        serde_json::to_string_pretty(&manifest)?.into_bytes(),
    );

    Ok(ArrayGraphSerializablePackage { manifest, blobs: b })
}

/// Restores an `ArrayGraphSerializable` from a manifest and blob data.
/// This is the exact inverse of the `to_blobs` function.
///
/// # Arguments
/// * `manifest` - The manifest containing metadata and blob IDs
/// * `blobs` - Map from BlobID to the actual blob data. This would be fetched from
///   the underlying storage where the graph was stored (db/filesystem/etc)
pub fn unpack(package: &ArrayGraphSerializablePackage) -> Result<ArrayGraphSerializable> {
    let ArrayGraphSerializableManifest {
        self_reference: _,
        stats: _,
        blobs,
        graph_settings,
    } = &package.manifest;

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

    let b = &package.blobs;

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
            let mut blob_id = BlobID(format!("{name}_{xx}"));
            if let Some(f) = cfg.modify_blob_id.as_ref() {
                blob_id = f(&blob_id.0);
            }
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
        include_str!("../../unigraph_core/src/tests/test_graphs/test_graph_2_left.json");

    fn make_graph() -> ArrayGraphSerializable {
        MapGraph::from_json(TEST_GRAPH_2)
            .unwrap()
            .to_array_graph_serializable()
            .unwrap()
    }

    #[test]
    fn serialize() -> Result<()> {
        let g = make_graph();

        let package = pack(
            &g,
            &StorageConfig {
                bytes_per_blob_chunk: Some(50),
                compression_level: Some(17),
                modify_blob_id: Some(Arc::new(|id| BlobID(id.to_string()))),
            },
        )?;

        snapshot!(
            serde_json::to_string_pretty(&package.manifest)?,
            r#"
{
  "self_reference": "_manifest.json",
  "stats": {
    "total_blobs": 13,
    "total_size_bytes": 397,
    "blob_sizes_bytes": {
      "directed_3098825159700367953": 35,
      "directed_offsets_17048802696332253084": 40,
      "dynamic_17709666951863227118": 50,
      "dynamic_3675328647461951329": 23,
      "entry_points_12265251058727778867": 13,
      "metrics_17201045065729657183": 30,
      "node_names_6281809031306549709": 27,
      "node_names_offsets_6491002063904830174": 43,
      "tag_sets_121953578755559923": 14,
      "tag_sets_16961032930212497945": 50,
      "tagged_10664201214824955125": 50,
      "tagged_8048188434168318281": 9,
      "traversal_config_12265251058727778867": 13
    },
    "node_count": 16,
    "directed_edge_count": 11
  },
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
            package
                .blobs
                .keys()
                .cloned()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
            r#"
[
    "_manifest.json",
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
        let package = pack(&original_graph, &StorageConfig::default())?;

        // Convert back from blobs
        let reconstructed_graph = unpack(&package)?;

        // Verify they're the same (by comparing JSON representations)
        let original_json = serde_json::to_string_pretty(&original_graph)?;
        let reconstructed_json = serde_json::to_string_pretty(&reconstructed_graph)?;

        assert_eq!(original_json, reconstructed_json);
        Ok(())
    }

    #[test]
    fn array_graph_serialization_perf_test() -> Result<()> {
        const TEST_GRAPH_PATH: &str = "/Users/dabramov/tmp/full_www_graph.json";

        // Only run the actual test if the graph is there. This is ment to run manually.
        if let Ok(graph_json) = std::fs::read_to_string(TEST_GRAPH_PATH) {
            let graph = MapGraph::from_json(&graph_json)?
                .to_array_graph_serializable()
                .context("Failed to convert to ArrayGraphSerializable")?;

            let time_now = std::time::Instant::now();
            let result = pack(
                &graph,
                &StorageConfig {
                    compression_level: Some(18),
                    ..Default::default()
                },
            )?;
            let duration = time_now.elapsed();
            eprintln!("Preparation for storage took: {duration:?}");
            drop(result);
            drop(graph);
        }

        Ok(())
    }
}

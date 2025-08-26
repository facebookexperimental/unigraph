// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use unigraph_serialization::ZSTDCompressionLevel;
use unigraph_serialization::from_base64;
use unigraph_serialization::from_zstd;
use unigraph_serialization::to_base_64;
use unigraph_serialization::to_zstd;
use xxhash_rust::xxh3::xxh3_64;

use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::ArrayGraphSerializableEdges;
use crate::ArrayGraphSerializableNodeMetadata;
use crate::graph_settings::GraphSettings;

const DEFAULT_BYTES_PER_BLOB_CHUNK: usize = 2_000_000; // 2 MB
const DEFAULT_COMPRESSION_LEVEL: ZSTDCompressionLevel = ZSTDCompressionLevel::Normal;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    typegen::TypeGen,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct BlobID(pub String);

/// ArrayGraphSerializable can be serialized and chunked into multiple compressed blobs
/// This manifest provides all the necessary metadata to locate and deserialize these blobs
/// back into the initial graph.
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializableManifest {
    /// Blob ID for the manifest itself serialized as JSON
    pub self_reference: BlobID,
    pub stats: ManifestStats,
    pub blobs: ManifestBlobs,

    pub graph_settings: Option<GraphSettings>,
}

/// Contains references to all individual blobs
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ManifestBlobs {
    pub node_names: Vec<BlobID>,
    pub node_names_offsets: Vec<BlobID>,

    /* EDGES */
    pub directed: Vec<BlobID>,
    pub directed_offsets: Vec<BlobID>,
    pub tagged: Vec<BlobID>,
    pub dynamic: Vec<BlobID>,

    /* METADATA */
    pub metrics: Vec<BlobID>,
    pub tag_sets: Vec<BlobID>,

    pub traversal_config: Vec<BlobID>,
    pub entry_points: Vec<BlobID>,
}

#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ManifestStats {
    pub total_blobs: u32,
    pub total_size_bytes: u32,
    pub blob_sizes_bytes: BTreeMap<BlobID, u32>,
    pub node_count: u32,
    pub directed_edge_count: u32,
}

#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializablePackage {
    pub manifest: ArrayGraphSerializableManifest,
    pub blobs: BTreeMap<BlobID, Vec<u8>>,
}

/// Base64 Version of the package, where the blobs are Base64 encoded.
/// This is intended for browser use or JSON encoding of the package itself
/// for passing between servers/clients.
/// If Vec<u8> is double serialized into JSON it will be represented as
///     [1, 19, 113, 48, ...]
/// which is very inefficient
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializablePackageBase64 {
    pub manifest: ArrayGraphSerializableManifest,
    pub blobs_base_64: BTreeMap<BlobID, String>,
}

impl ManifestBlobs {
    pub fn get_all_blob_ids(&self) -> Vec<BlobID> {
        let Self {
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
        } = self;

        [
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
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect()
    }
}

impl From<String> for BlobID {
    fn from(s: String) -> Self {
        BlobID(s)
    }
}

impl From<&str> for BlobID {
    fn from(s: &str) -> Self {
        BlobID(s.to_string())
    }
}

impl From<BlobID> for String {
    fn from(blob_id: BlobID) -> Self {
        blob_id.0
    }
}

impl ManifestStats {
    pub fn from_blobs(blobs: &BTreeMap<BlobID, Vec<u8>>, graph: &ArrayGraphSerializable) -> Self {
        let total_blobs = blobs.len() as u32;
        let total_size_bytes = blobs.values().map(|b| b.len()).sum::<usize>() as u32;
        let blob_sizes_bytes = blobs
            .iter()
            .map(|(k, v)| (k.clone(), v.len() as u32))
            .collect();
        Self {
            total_blobs,
            total_size_bytes,
            blob_sizes_bytes,
            node_count: graph.node_names_ordered.combined_nodes_len() as u32,
            directed_edge_count: graph.edges.directed.len() as u32,
        }
    }
}

type ModifyBlobID = Option<Arc<dyn Fn(&str) -> BlobID + Send + Sync>>;

#[derive(Default, Clone)]
pub struct ArrayGraphSerializablePackageConfig {
    pub bytes_per_blob_chunk: Option<usize>,

    /// ZSTD compression level
    pub compression_level: Option<ZSTDCompressionLevel>,

    /// A function that generates an ID for the blob.
    /// This is here to decouple the logic of preparing the graph
    /// for storage from the blobstorage implementation.
    /// e.g. in Manifold we're likely gonna store graphs
    /// under a certain namespace and a GraphID folder, which
    /// should not be a concern of the logic in this file.
    pub modify_blob_id: ModifyBlobID,
}

impl ArrayGraphSerializablePackageConfig {
    pub fn bytes_per_chunk(&self) -> usize {
        self.bytes_per_blob_chunk
            .unwrap_or(DEFAULT_BYTES_PER_BLOB_CHUNK)
    }

    pub fn compression_level(&self) -> ZSTDCompressionLevel {
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
    c: &ArrayGraphSerializablePackageConfig,
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

    let node_names = into_blobs(&node_names, "node_names", &mut b, c)?;
    let node_names_offsets = into_blobs(&node_names_offsets, "node_names_offsets", &mut b, c)?;
    let directed = into_blobs(&directed, "directed", &mut b, c)?;
    let directed_offsets = into_blobs(&directed_offsets, "directed_offsets", &mut b, c)?;
    let tagged = into_blobs(&tagged, "tagged", &mut b, c)?;
    let dynamic = into_blobs(&dynamic, "dynamic", &mut b, c)?;
    let metrics = into_blobs(&metrics, "metrics", &mut b, c)?;
    let tag_sets = into_blobs(&tag_sets, "tag_sets", &mut b, c)?;
    let traversal_config = into_blobs(&traversal_config, "traversal_config", &mut b, c)?;
    let entry_points = into_blobs(&entry_points, "entry_points", &mut b, c)?;

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
    (|| {
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

        anyhow::Ok(ArrayGraphSerializable {
            node_names_ordered: Arc::new(ArrayGraphNodes::from_parts(
                node_names,
                node_name_offsets,
            )),
            edges,
            node_metadata,
            graph_settings: graph_settings.clone(),
            traversal_config,
            entry_points,
        })
    })()
    .context("Failed to unpack graph")
    .with_context(|| {
        format!(
            "Manifest: {}",
            serde_json::to_string_pretty(&package.manifest).unwrap()
        )
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
        let json = from_zstd(&combined_data)?;

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

pub fn into_blobs<T: serde::Serialize>(
    value: &T,
    name: &str,
    all_blobs: &mut BTreeMap<BlobID, Vec<u8>>,
    cfg: &ArrayGraphSerializablePackageConfig,
) -> Result<Vec<BlobID>> {
    let json = serde_json::to_vec(value)?;
    let zstd = to_zstd(&json, cfg.compression_level())?;

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

impl ArrayGraphSerializablePackage {
    pub fn into_base_64(self) -> ArrayGraphSerializablePackageBase64 {
        let ArrayGraphSerializablePackage { manifest, blobs } = self;
        let blobs_base_64 = blobs
            .into_iter()
            .map(|(k, v)| (k, to_base_64(&v)))
            .collect();

        ArrayGraphSerializablePackageBase64 {
            manifest,
            blobs_base_64,
        }
    }

    pub fn from_base64(package_base_64: ArrayGraphSerializablePackageBase64) -> Result<Self> {
        let ArrayGraphSerializablePackageBase64 {
            manifest,
            blobs_base_64,
        } = package_base_64;
        Ok(ArrayGraphSerializablePackage {
            manifest,
            blobs: blobs_base_64
                .iter()
                .map(|(k, v)| Ok((k.clone(), from_base64(v)?)))
                .collect::<Result<_>>()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::MapGraph;
    use crate::tests::test_graphs::make_test_array_graph_2;

    #[test]
    fn serialize() -> Result<()> {
        let g = make_test_array_graph_2()?.into_serializable();

        let package = pack(
            &g,
            &ArrayGraphSerializablePackageConfig {
                bytes_per_blob_chunk: Some(50),
                compression_level: Some(ZSTDCompressionLevel::Best),
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
      "directed_1506826171969472540": 35,
      "directed_offsets_8316678694188447186": 40,
      "dynamic_3675328647461951329": 23,
      "dynamic_768470073201454812": 50,
      "entry_points_9535545603450022154": 13,
      "metrics_6304071051133242967": 30,
      "node_names_10311418653884441124": 27,
      "node_names_offsets_15446562321729131330": 43,
      "tag_sets_121953578755559923": 14,
      "tag_sets_2696313957685523905": 50,
      "tagged_3600822166880560972": 50,
      "tagged_8048188434168318281": 9,
      "traversal_config_9535545603450022154": 13
    },
    "node_count": 16,
    "directed_edge_count": 11
  },
  "blobs": {
    "node_names": [
      "node_names_10311418653884441124"
    ],
    "node_names_offsets": [
      "node_names_offsets_15446562321729131330"
    ],
    "directed": [
      "directed_1506826171969472540"
    ],
    "directed_offsets": [
      "directed_offsets_8316678694188447186"
    ],
    "tagged": [
      "tagged_3600822166880560972",
      "tagged_8048188434168318281"
    ],
    "dynamic": [
      "dynamic_3675328647461951329",
      "dynamic_768470073201454812"
    ],
    "metrics": [
      "metrics_6304071051133242967"
    ],
    "tag_sets": [
      "tag_sets_121953578755559923",
      "tag_sets_2696313957685523905"
    ],
    "traversal_config": [
      "traversal_config_9535545603450022154"
    ],
    "entry_points": [
      "entry_points_9535545603450022154"
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
    "directed_1506826171969472540",
    "directed_offsets_8316678694188447186",
    "dynamic_3675328647461951329",
    "dynamic_768470073201454812",
    "entry_points_9535545603450022154",
    "metrics_6304071051133242967",
    "node_names_10311418653884441124",
    "node_names_offsets_15446562321729131330",
    "tag_sets_121953578755559923",
    "tag_sets_2696313957685523905",
    "tagged_3600822166880560972",
    "tagged_8048188434168318281",
    "traversal_config_9535545603450022154",
]
"#
        );
        Ok(())
    }

    #[test]
    fn roundtrip_to_blobs_and_from_blobs() -> Result<()> {
        let original_graph = make_test_array_graph_2()?.into_serializable();

        // Convert to blobs
        let package = pack(
            &original_graph,
            &ArrayGraphSerializablePackageConfig::default(),
        )?;

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
                &ArrayGraphSerializablePackageConfig {
                    compression_level: Some(ZSTDCompressionLevel::Best),
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Chunked, compressed blob packaging for [`ArrayGraphSerializable`].
//!
//! A graph is serialized by converting each field (node names, edges, metrics,
//! etc.) to JSON, compressing with ZSTD, and splitting into fixed-size chunks.
//! The result is a set of content-addressed blobs plus a [`ArrayGraphSerializableManifest`]
//! that records which blobs belong to which field.
//!
//! ## Pack / Unpack
//!
//! - [`pack`] — serialize a graph into blobs + manifest.
//! - [`unpack`] — reconstruct a graph from blobs + manifest.
//!
//! ## Base64
//!
//! [`ArrayGraphSerializablePackageBase64`] provides a JSON-friendly encoding
//! where each blob is base64-encoded instead of stored as raw bytes.

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
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::array_graph::budget_graph::BudgetConfig;

/// Default maximum size of each blob chunk before splitting (2 MB).
const DEFAULT_BYTES_PER_BLOB_CHUNK: usize = 2_000_000; // 2 MB

/// Default ZSTD compression level used when packing graphs.
const DEFAULT_COMPRESSION_LEVEL: ZSTDCompressionLevel = ZSTDCompressionLevel::Normal;

/// Unique identifier for a blob within a graph package.
///
/// Each blob holds a compressed chunk of serialized graph data (e.g. node names,
/// edges, metrics). The ID is typically derived from the field name and a hash
/// of the blob contents (e.g. `"directed_1506826171969472540"`).
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

/// Metadata about the blob layout of a serialized graph.
///
/// When an [`ArrayGraphSerializable`] is packed via [`pack`], it is split into
/// individually compressed blobs. This manifest records which [`BlobID`]s
/// correspond to which graph fields, along with size statistics, so that the
/// graph can later be reassembled from blob storage without loading the
/// entire dataset at once.
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializableManifest {
    /// [`BlobID`] that points to the JSON-serialized manifest itself, so the
    /// manifest can be stored alongside the other blobs in the same store.
    pub self_reference: BlobID,
    /// Aggregate statistics about the package (total size, blob count, etc.).
    pub stats: ManifestStats,
    /// Per-field blob references that map each graph component to its blob(s).
    pub blobs: ManifestBlobs,

    /// Optional graph-level settings (e.g. display configuration).
    pub graph_settings: Option<GraphSettings>,

    /// Graph-level key-value properties (not per-node).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

/// Maps each logical field of the graph to the [`BlobID`](s) that hold its
/// compressed data. A field may span multiple blobs if it exceeds the
/// configured chunk size.
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ManifestBlobs {
    /// Concatenated UTF-8 node name bytes.
    pub node_names: Vec<BlobID>,
    /// Offsets into the `node_names` byte array (one per node).
    pub node_names_offsets: Vec<BlobID>,

    /* EDGES */
    /// Flat list of directed-edge target indices (paired with `directed_offsets`).
    pub directed: Vec<BlobID>,
    /// CSR-style offsets into `directed` (one per source node + 1 sentinel).
    pub directed_offsets: Vec<BlobID>,
    /// Tagged edges (node → tag → set of target nodes).
    pub tagged: Vec<BlobID>,
    /// Dynamic edges with runtime-defined branch labels.
    pub dynamic: Vec<BlobID>,

    /* METADATA */
    /// Per-metric float vectors (one `f32` per node for each named metric).
    pub metrics: Vec<BlobID>,
    /// Per-label-name index (label-name → node → set of values).
    pub labels: Vec<BlobID>,
    /// Per-property-name index (property-name → node → value).
    #[serde(default)]
    pub properties: Vec<BlobID>,

    /// Optional traversal configuration (entry points, tier rules, etc.).
    pub traversal_config: Vec<BlobID>,
    /// Budget configurations keyed by project name.
    #[serde(default)]
    pub budget_configs: Vec<BlobID>,
    /// Explicit graph entry points, if set.
    pub entry_points: Vec<BlobID>,

    /// Graph-level key-value properties (stored in manifest, not as blobs).
    #[serde(default)]
    pub graph_properties: Vec<BlobID>,
}

/// Summary statistics recorded at pack time.
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ManifestStats {
    /// Total number of data blobs (excludes the manifest blob itself).
    pub total_blobs: u32,
    /// Sum of all blob sizes in bytes (compressed).
    pub total_size_bytes: u32,
    /// Per-blob compressed size map.
    pub blob_sizes_bytes: BTreeMap<BlobID, u32>,
    /// Number of nodes in the graph.
    pub node_count: u32,
    /// Number of directed edges in the graph.
    pub directed_edge_count: u32,
}

/// A fully self-contained graph package: the manifest plus all blob data.
///
/// Blobs are stored as raw `Vec<u8>` (ZSTD-compressed). This representation
/// is suitable for in-memory use or binary serialization.
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializablePackage {
    pub manifest: ArrayGraphSerializableManifest,
    pub blobs: BTreeMap<BlobID, Vec<u8>>,
}

/// Base64-encoded variant of [`ArrayGraphSerializablePackage`].
///
/// JSON-serializing raw `Vec<u8>` produces a verbose integer array
/// (`[1, 19, 113, ...]`). This variant stores each blob as a base64
/// string instead, making it much more compact for JSON transport
/// between servers and browsers.
#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializablePackageBase64 {
    pub manifest: ArrayGraphSerializableManifest,
    pub blobs_base_64: BTreeMap<BlobID, String>,
}

impl ManifestBlobs {
    /// Returns a flat list of every [`BlobID`] referenced by this manifest,
    /// across all fields, in a deterministic order.
    pub fn get_all_blob_ids(&self) -> Vec<BlobID> {
        let Self {
            node_names,
            node_names_offsets,
            directed,
            directed_offsets,
            tagged,
            dynamic,
            metrics,
            labels,
            properties,
            traversal_config,
            budget_configs,
            entry_points,
            graph_properties,
        } = self;

        [
            node_names,
            node_names_offsets,
            directed,
            directed_offsets,
            tagged,
            dynamic,
            metrics,
            labels,
            properties,
            traversal_config,
            budget_configs,
            entry_points,
            graph_properties,
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
    /// Computes statistics from the current set of blobs and the source graph.
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

/// Callback type used to transform blob IDs before they are stored.
///
/// This decouples the packing logic from the storage layer — for example,
/// in Manifold-backed storage the callback can prepend a namespace and
/// graph ID prefix to each raw blob ID.
type ModifyBlobID = Option<Arc<dyn Fn(&str) -> BlobID + Send + Sync>>;

/// Configuration for [`pack`] controlling chunking, compression, and blob ID
/// generation.
#[derive(Default, Clone)]
pub struct ArrayGraphSerializablePackageConfig {
    /// Maximum number of bytes per blob chunk. Larger fields are split into
    /// multiple blobs at this boundary. Defaults to [`DEFAULT_BYTES_PER_BLOB_CHUNK`] (2 MB).
    pub bytes_per_blob_chunk: Option<usize>,

    /// ZSTD compression level applied to each blob.
    /// Defaults to [`DEFAULT_COMPRESSION_LEVEL`].
    pub compression_level: Option<ZSTDCompressionLevel>,

    /// Optional callback that transforms blob IDs before they are stored.
    ///
    /// This decouples packing logic from the storage backend — for example,
    /// prepending a namespace or graph ID prefix for Manifold storage.
    pub modify_blob_id: ModifyBlobID,
}

impl ArrayGraphSerializablePackageConfig {
    /// Returns the configured chunk size or the default (2 MB).
    pub fn bytes_per_chunk(&self) -> usize {
        self.bytes_per_blob_chunk
            .unwrap_or(DEFAULT_BYTES_PER_BLOB_CHUNK)
    }

    /// Returns the configured compression level or the default.
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
        budget_configs,
        entry_points,
        properties: graph_properties,
    } = &graph;

    let ArrayGraphSerializableEdges {
        directed,
        directed_offsets,
        tagged,
        dynamic,
    } = &edges;

    let ArrayGraphSerializableNodeMetadata {
        metrics,
        labels,
        properties,
    } = &node_metadata;

    let (node_names, node_names_offsets) = node_names_ordered.as_parts();

    // Serialize + compress all fields in parallel via rayon::scope.
    // Each field runs on its own thread; results are collected by name.
    // On WASM (no thread pool), rayon degrades to sequential automatically.
    type FieldBlobs = Result<(Vec<BlobID>, BTreeMap<BlobID, Vec<u8>>)>;
    let results = std::sync::Mutex::new(BTreeMap::<&str, FieldBlobs>::new());

    macro_rules! spawn_field {
        ($s:expr, $($field:ident),+ $(,)?) => {$(
            $s.spawn(|_| {
                let r = into_blobs_isolated(&$field, stringify!($field), c);
                results.lock().unwrap().insert(stringify!($field), r);
            });
        )+};
    }

    rayon::scope(|s| {
        spawn_field!(
            s,
            node_names,
            node_names_offsets,
            directed,
            directed_offsets,
            tagged,
            dynamic,
            metrics,
            labels,
            properties,
            traversal_config,
            budget_configs,
            entry_points,
        );
    });

    let mut results = results
        .into_inner()
        .map_err(|e| anyhow::anyhow!("rayon task panicked: {e}"))?;

    macro_rules! take_field {
        ($field:ident) => {{
            let (ids, blobs) = results
                .remove(stringify!($field))
                .context(concat!("missing result for field: ", stringify!($field)))??;
            b.extend(blobs);
            ids
        }};
    }

    let manifest_blobs = ManifestBlobs {
        node_names: take_field!(node_names),
        node_names_offsets: take_field!(node_names_offsets),
        directed: take_field!(directed),
        directed_offsets: take_field!(directed_offsets),
        tagged: take_field!(tagged),
        dynamic: take_field!(dynamic),
        metrics: take_field!(metrics),
        labels: take_field!(labels),
        properties: take_field!(properties),
        traversal_config: take_field!(traversal_config),
        budget_configs: take_field!(budget_configs),
        entry_points: take_field!(entry_points),
        graph_properties: vec![],
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
        properties: graph_properties.clone(),
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
            properties: graph_properties,
        } = &package.manifest;

        let ManifestBlobs {
            node_names,
            node_names_offsets,
            directed,
            directed_offsets,
            tagged,
            dynamic,
            metrics,
            labels,
            properties,
            traversal_config,
            budget_configs,
            entry_points,
            graph_properties: _, // stored in manifest directly, not as blobs
        } = &blobs;

        let b = &package.blobs;

        // Decompress + deserialize all fields in parallel via rayon::scope.
        // Mirrors the parallel serialization in `pack()`.
        // Each slot holds the result for one field; rayon tasks write into them.
        macro_rules! field_slot {
            () => {
                std::sync::Mutex::new(None)
            };
        }

        let r_node_names = field_slot!();
        let r_node_name_offsets = field_slot!();
        let r_directed = field_slot!();
        let r_directed_offsets = field_slot!();
        let r_tagged = field_slot!();
        let r_dynamic = field_slot!();
        let r_metrics = field_slot!();
        let r_labels = field_slot!();
        let r_properties = field_slot!();
        let r_traversal_config = field_slot!();
        let r_budget_configs = field_slot!();
        let r_entry_points = field_slot!();

        rayon::scope(|s| {
            s.spawn(|_| {
                *r_node_names.lock().unwrap() = Some(from_blobs_field(node_names, b));
            });
            s.spawn(|_| {
                *r_node_name_offsets.lock().unwrap() =
                    Some(from_blobs_field(node_names_offsets, b));
            });
            s.spawn(|_| {
                *r_directed.lock().unwrap() = Some(from_blobs_field(directed, b));
            });
            s.spawn(|_| {
                *r_directed_offsets.lock().unwrap() = Some(from_blobs_field(directed_offsets, b));
            });
            s.spawn(|_| {
                *r_tagged.lock().unwrap() = Some(from_blobs_field(tagged, b));
            });
            s.spawn(|_| {
                *r_dynamic.lock().unwrap() = Some(from_blobs_field(dynamic, b));
            });
            s.spawn(|_| {
                *r_metrics.lock().unwrap() = Some(from_blobs_field(metrics, b));
            });
            s.spawn(|_| {
                *r_labels.lock().unwrap() = Some(from_blobs_field(labels, b));
            });
            s.spawn(|_| {
                *r_properties.lock().unwrap() = Some(if properties.is_empty() {
                    Ok(BTreeMap::new())
                } else {
                    from_blobs_field(properties, b)
                });
            });
            s.spawn(|_| {
                *r_traversal_config.lock().unwrap() = Some(from_blobs_field(traversal_config, b));
            });
            s.spawn(|_| {
                *r_budget_configs.lock().unwrap() = Some(if budget_configs.is_empty() {
                    Ok(BTreeMap::new())
                } else {
                    from_blobs_field(budget_configs, b)
                });
            });
            s.spawn(|_| {
                *r_entry_points.lock().unwrap() = Some(from_blobs_field(entry_points, b));
            });
        });

        // Extract results — type inference works because each slot feeds
        // directly into the struct field that determines its type.
        macro_rules! take {
            ($slot:ident) => {
                $slot
                    .into_inner()
                    .unwrap()
                    .expect(concat!("missing result for ", stringify!($slot)))?
            };
        }

        let node_names = take!(r_node_names);
        let node_name_offsets = take!(r_node_name_offsets);
        let directed = take!(r_directed);
        let directed_offsets = take!(r_directed_offsets);
        let tagged = take!(r_tagged);
        let dynamic = take!(r_dynamic);
        let metrics = take!(r_metrics);
        let labels = take!(r_labels);
        let properties = take!(r_properties);
        let traversal_config = take!(r_traversal_config);
        let budget_configs = take!(r_budget_configs);
        let entry_points = take!(r_entry_points);

        let edges = ArrayGraphSerializableEdges {
            directed,
            directed_offsets,
            tagged,
            dynamic,
        };

        let node_metadata = ArrayGraphSerializableNodeMetadata {
            metrics,
            labels,
            properties,
        };

        anyhow::Ok(ArrayGraphSerializable {
            node_names_ordered: Arc::new(ArrayGraphNodes::from_parts(
                node_names,
                node_name_offsets,
            )),
            edges,
            node_metadata,
            graph_settings: graph_settings.clone(),
            traversal_config,
            budget_configs,
            entry_points,
            properties: graph_properties.clone(),
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

/// Reconstructs a single graph field from one or more compressed blobs.
///
/// The blobs are concatenated in order, ZSTD-decompressed, and then
/// deserialized from JSON back into `T`. Returns `T::default()` implicitly
/// through deserialization if the data is empty.
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

/// Like [`into_blobs`], but returns the blob map instead of mutating a shared one.
///
/// This enables parallel serialization — each field can produce its own blob map
/// independently, and results are merged after all tasks complete.
fn into_blobs_isolated<T: serde::Serialize>(
    value: &T,
    name: &str,
    cfg: &ArrayGraphSerializablePackageConfig,
) -> Result<(Vec<BlobID>, BTreeMap<BlobID, Vec<u8>>)> {
    let mut blobs = BTreeMap::new();
    let ids = into_blobs(value, name, &mut blobs, cfg)?;
    Ok((ids, blobs))
}

/// Serializes a single graph field into one or more compressed blobs.
///
/// The value is JSON-serialized, ZSTD-compressed, and then split into chunks
/// of at most `cfg.bytes_per_chunk()` bytes. Each chunk is assigned a
/// [`BlobID`] derived from the field `name` and an xxHash of its contents.
/// The blobs are inserted into `all_blobs` and the corresponding IDs are
/// returned in order.
pub fn into_blobs<T: serde::Serialize>(
    value: &T,
    name: &str,
    all_blobs: &mut BTreeMap<BlobID, Vec<u8>>,
    cfg: &ArrayGraphSerializablePackageConfig,
) -> Result<Vec<BlobID>> {
    let json = serde_json::to_vec(value)?;
    let zstd = to_zstd(&json, cfg.compression_level())?;

    let chunks = into_chunks(zstd, cfg.bytes_per_chunk());
    let multiple_chunks = chunks.len() > 1;
    let result: Vec<(BlobID, Vec<u8>)> = chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let xx = xxh3_64(&chunk);
            let chunk_suffix = if multiple_chunks {
                format!("_chunk_{i}")
            } else {
                String::new()
            };

            let mut blob_id = BlobID(format!("{name}{chunk_suffix}_{xx}"));
            if let Some(f) = cfg.modify_blob_id.as_ref() {
                blob_id = f(&blob_id.0);
            }
            (blob_id, chunk)
        })
        .collect();

    let ids = result.iter().map(|(id, _)| id.clone()).collect();
    all_blobs.extend(result);
    Ok(ids)
}

/// Splits `blob` into sequential chunks of at most `chunk_size_bytes`.
/// The last chunk may be smaller. Always returns at least one chunk (even if
/// `blob` is empty).
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
    /// Converts all blobs to base64 strings for JSON-friendly transport.
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

    /// Constructs a package from a base64-encoded variant by decoding each blob.
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
    "total_blobs": 15,
    "total_size_bytes": 407,
    "blob_sizes_bytes": {
      "budget_configs_4370653166743570923": 11,
      "directed_1506826171969472540": 35,
      "directed_offsets_8316678694188447186": 40,
      "dynamic_chunk_0_16704539601918712447": 50,
      "dynamic_chunk_1_14093561304655809570": 11,
      "entry_points_9535545603450022154": 13,
      "labels_chunk_0_13613338088011413788": 50,
      "labels_chunk_1_15517762289522568128": 14,
      "metrics_6304071051133242967": 30,
      "node_names_10311418653884441124": 27,
      "node_names_offsets_15446562321729131330": 43,
      "properties_4370653166743570923": 11,
      "tagged_chunk_0_3600822166880560972": 50,
      "tagged_chunk_1_8048188434168318281": 9,
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
      "tagged_chunk_0_3600822166880560972",
      "tagged_chunk_1_8048188434168318281"
    ],
    "dynamic": [
      "dynamic_chunk_0_16704539601918712447",
      "dynamic_chunk_1_14093561304655809570"
    ],
    "metrics": [
      "metrics_6304071051133242967"
    ],
    "labels": [
      "labels_chunk_0_13613338088011413788",
      "labels_chunk_1_15517762289522568128"
    ],
    "properties": [
      "properties_4370653166743570923"
    ],
    "traversal_config": [
      "traversal_config_9535545603450022154"
    ],
    "budget_configs": [
      "budget_configs_4370653166743570923"
    ],
    "entry_points": [
      "entry_points_9535545603450022154"
    ],
    "graph_properties": []
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
    "budget_configs_4370653166743570923",
    "directed_1506826171969472540",
    "directed_offsets_8316678694188447186",
    "dynamic_chunk_0_16704539601918712447",
    "dynamic_chunk_1_14093561304655809570",
    "entry_points_9535545603450022154",
    "labels_chunk_0_13613338088011413788",
    "labels_chunk_1_15517762289522568128",
    "metrics_6304071051133242967",
    "node_names_10311418653884441124",
    "node_names_offsets_15446562321729131330",
    "properties_4370653166743570923",
    "tagged_chunk_0_3600822166880560972",
    "tagged_chunk_1_8048188434168318281",
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

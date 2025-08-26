// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use crate::ArrayGraphSerializable;
use crate::graph_settings::GraphSettings;

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

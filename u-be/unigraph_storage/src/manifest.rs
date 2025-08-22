// Copyright (c) Meta Platforms, Inc. and affiliates.

use unigraph_core::graph_settings::GraphSettings;

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

#[derive(Debug, typegen::TypeGen, serde::Serialize, serde::Deserialize)]
pub struct ArrayGraphSerializableManifest {
    pub blobs: ManifestBlobs,
    pub graph_settings: Option<GraphSettings>,
}

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

impl From<String> for BlobID {
    fn from(s: String) -> Self {
        BlobID(s)
    }
}

impl From<BlobID> for String {
    fn from(blob_id: BlobID) -> Self {
        blob_id.0
    }
}

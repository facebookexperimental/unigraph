// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Chunked, compressed blob packaging for error data.
//!
//! Error messages from failed graph computations can be very long, so they
//! go through the same blob pipeline as graphs and deltas: JSON-serialize,
//! ZSTD-compress, chunk, and content-address.
//!
//! ## Pack / Unpack
//!
//! - [`pack_errors`] — serialize error data into blobs + manifest.
//! - [`unpack_errors`] — reconstruct error data from blobs + manifest.
//!
//! The error data type is generic (`T: Serialize + DeserializeOwned`) so this
//! module has no dependency on specific error types defined in other crates.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

use crate::array_graph_serializable::package::ArrayGraphSerializablePackageConfig;
use crate::array_graph_serializable::package::BlobID;
use crate::array_graph_serializable::package::from_blobs_json;
use crate::array_graph_serializable::package::into_blobs_json;

/// Manifest for packaged error data.
///
/// Stores metadata about the errors and references to the blob(s)
/// containing the serialized error list.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorManifest {
    /// Blob ID that points to the JSON-serialized manifest itself.
    pub self_reference: BlobID,
    /// Statistics about the error package.
    pub stats: ErrorManifestStats,
    /// Blob IDs containing the serialized error data (chunked if large).
    pub errors_blob: Vec<BlobID>,
}

/// Statistics about a packaged error set.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorManifestStats {
    /// Total number of data blobs (excludes the manifest blob itself).
    pub total_blobs: u32,
    /// Sum of all blob sizes in bytes (compressed).
    pub total_size_bytes: u32,
    /// Per-blob compressed size map.
    pub blob_sizes_bytes: BTreeMap<BlobID, u32>,
    /// Number of errors in the package.
    pub error_count: u32,
}

/// A fully self-contained error package: the manifest plus all blob data.
pub struct ErrorPackage {
    pub manifest: ErrorManifest,
    pub blobs: BTreeMap<BlobID, Vec<u8>>,
}

impl ErrorManifestStats {
    fn from_blobs(blobs: &BTreeMap<BlobID, Vec<u8>>, error_count: u32) -> Self {
        let total_blobs = blobs.len() as u32;
        let total_size_bytes = blobs.values().map(|b| b.len()).sum::<usize>() as u32;
        let blob_sizes_bytes = blobs
            .iter()
            .map(|(k, v)| (k.clone(), v.len() as u32))
            .collect();

        ErrorManifestStats {
            total_blobs,
            total_size_bytes,
            blob_sizes_bytes,
            error_count,
        }
    }
}

/// Pack error data into a manifest + blobs.
///
/// The errors are serialized as JSON, ZSTD-compressed, and split into
/// content-addressed chunks using the same infrastructure as graph packaging.
///
/// The generic type `T` is typically `Vec<TimestampedError>` but can be
/// any serializable type.
pub fn pack_errors<T: serde::Serialize + Sync>(
    errors: &T,
    error_count: u32,
    config: &ArrayGraphSerializablePackageConfig,
) -> Result<ErrorPackage> {
    let mut blobs = BTreeMap::new();

    let errors_blob = into_blobs_json(errors, "errors", &mut blobs, config)?;

    let mut manifest_blob_id = BlobID::from("_error_manifest.json");
    if let Some(f) = config.modify_blob_id.as_ref() {
        manifest_blob_id = f(&manifest_blob_id.0);
    }

    let stats = ErrorManifestStats::from_blobs(&blobs, error_count);

    let manifest = ErrorManifest {
        self_reference: manifest_blob_id.clone(),
        stats,
        errors_blob,
    };

    blobs.insert(
        manifest_blob_id,
        serde_json::to_string_pretty(&manifest)?.into_bytes(),
    );

    Ok(ErrorPackage { manifest, blobs })
}

/// Unpack an [`ErrorPackage`] back into the original error data.
pub fn unpack_errors<T: serde::de::DeserializeOwned + Default + Send>(
    package: &ErrorPackage,
) -> Result<T> {
    let task = ll::Task::create_new("");
    from_blobs_json(&package.manifest.errors_blob, &package.blobs, &task)
        .context("Failed to unpack errors")
        .with_context(|| {
            format!(
                "ErrorManifest: {}",
                serde_json::to_string_pretty(&package.manifest).unwrap_or_default()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_error_pack_unpack() -> Result<()> {
        let errors = vec![
            "Error 1: something went wrong".to_string(),
            "Error 2: another failure".to_string(),
            "Error 3: a very long error message that could potentially be quite large in practice"
                .to_string(),
        ];

        let package = pack_errors(
            &errors,
            errors.len() as u32,
            &ArrayGraphSerializablePackageConfig::default(),
        )?;

        assert_eq!(package.manifest.stats.error_count, 3);

        let roundtripped: Vec<String> = unpack_errors(&package)?;
        assert_eq!(errors, roundtripped);

        Ok(())
    }
}

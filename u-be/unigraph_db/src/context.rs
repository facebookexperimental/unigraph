// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Shared database context — provides storage and pack configuration
//! to all namespace handles.

use std::sync::Arc;

use rand::RngExt as _;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::BlobID;
use unigraph_storage_core::GraphTimeKey;

use crate::storage::UnigraphStorage;

/// Default threshold for storing config blobs externally (5 KB).
pub const DEFAULT_CONFIG_INLINE_BLOB_THRESHOLD: usize = 5_000;

/// Shared context for all database operations.
///
/// Holds the storage compositor and base pack configuration. Passed to
/// all namespace handles so they have access to everything they need.
///
/// `Clone` is cheap (the heavy storage backend is behind `Arc`).
#[derive(Clone)]
pub(crate) struct UnigraphDbContext {
    pub storage: Arc<UnigraphStorage>,
    pub base_pack_config: ArrayGraphSerializablePackageConfig,
    /// Config blobs larger than this (in bytes) are stored in external blob
    /// storage instead of inline in the configs table.
    pub config_inline_blob_threshold: usize,
}

impl UnigraphDbContext {
    /// Create a per-frame pack config that gives every blob a unique ID.
    ///
    /// Blob IDs come out as `graphs/{timeline_id}/{graph_id}/{blob}_{random}`,
    /// where `blob` says what the thing is (`csr_edges_chunk_3`,
    /// `_delta_manifest.json`) and `random` is 64 fresh bits per blob.
    ///
    /// # Why the random suffix
    ///
    /// Without it a blob ID is a pure function of the frame and its contents,
    /// so two writers packing the same graph at the same `graph_id` produce
    /// byte-identical IDs. That breaks the crash-safe store path, which
    /// registers a blob key for cleanup *before* uploading and unregisters it
    /// only on commit, on the stated assumption that "if my transaction rolls
    /// back, my blobs are orphans". With shared IDs the assumption is false:
    /// the loser's rollback leaves the *winner's* live blobs queued, and the
    /// sweeper deletes them out from under a committed frame. No race window is
    /// needed either — a store re-packing a frame that some earlier delete
    /// already queued lands in the same place.
    ///
    /// Randomness rather than a counter or a timestamp because the writers
    /// cannot coordinate: different hosts, different jobs, no shared state. The
    /// cases that collide are exactly the ones that start together.
    ///
    /// # Why every blob and not one token per store
    ///
    /// A token per store would do — uniqueness is all that is required, and a
    /// shared token would group an attempt's blobs together. It is not worth a
    /// second concept: per blob is one `format!`, needs nothing threaded
    /// through `unigraph_core`, and an abandoned attempt is already enumerable
    /// from the cleanup queue rather than by eyeballing prefixes.
    ///
    /// # The suffix must reach the manifests too
    ///
    /// This is the part that makes `modify_blob_id` the only correct home for
    /// it. Chunk IDs have a natural slot for a suffix, but `_manifest.json`,
    /// `_delta_manifest.json` and `_error_manifest.json` are fixed names with
    /// no slot at all. Randomising only the chunk IDs would leave two
    /// concurrent writers colliding on the manifest — the single blob that
    /// names all the others. `modify_blob_id` is applied to chunk IDs and
    /// manifest IDs alike, so putting it here covers both by construction.
    ///
    /// Renaming the manifest is safe because nothing reconstructs its key by
    /// convention: `store_package_on_conn` keeps the authoritative manifest in
    /// the frame row's `manifest_json` column, and the stored blob's key is
    /// whatever `self_reference` inside it says. The blob is a self-describing
    /// copy, never something the reader goes looking for by name.
    ///
    /// The cost is that a failed store leaks a full set of blobs instead of
    /// being overwritten in place by the retry, so this leans harder on the
    /// sweeper actually working.
    pub fn pack_config_for_key(&self, key: &GraphTimeKey) -> ArrayGraphSerializablePackageConfig {
        let mut config = self.base_pack_config.clone();
        let timeline_id = key.timeline_id.clone();
        let graph_id = key.graph_id;
        config.modify_blob_id = Some(Arc::new(move |id| {
            BlobID(format!(
                "graphs/{}/{}/{}",
                timeline_id.0,
                graph_id.0,
                with_random_suffix(id)
            ))
        }));
        config
    }
}

/// Append 64 random bits to a blob ID, before the extension if it has one.
///
/// `csr_edges_chunk_3` -> `csr_edges_chunk_3_1f4a09c6b73e5d82`
/// `_delta_manifest.json` -> `_delta_manifest_1f4a09c6b73e5d82.json`
///
/// Only the manifests carry an extension, and keeping `.json` on the end of
/// them is worth the two lines: it is what tells you the blob is readable
/// without fetching it.
fn with_random_suffix(id: &str) -> String {
    let random = rand::rng().random::<u64>();
    match id.strip_suffix(JSON_EXTENSION) {
        Some(stem) => format!("{stem}_{random:016x}{JSON_EXTENSION}"),
        None => format!("{id}_{random:016x}"),
    }
}

const JSON_EXTENSION: &str = ".json";

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
    /// Create a per-frame pack config with blob ID prefixing.
    ///
    /// Clones the base config and sets `modify_blob_id` to prefix all blob IDs
    /// with `graphs/{timeline_id}/{graph_id}/{attempt}/`, where `attempt` is a
    /// fresh random token per call — that is, per packed frame, since every
    /// call site uses the returned config for exactly one `pack`.
    ///
    /// # Why the attempt token exists
    ///
    /// Blob IDs are content-addressed (`xxh3_64` of the compressed chunk) and
    /// the prefix is per frame, so without the token two writers packing the
    /// *same* graph at the *same* `graph_id` produce byte-identical keys. That
    /// collides with the crash-safe store path in the worst possible way. That
    /// path registers a blob key for cleanup *before* uploading it and
    /// unregisters it only on commit, on the stated assumption that "if my
    /// transaction rolls back, my blobs are orphans". With shared keys the
    /// assumption is false: the loser's rollback leaves the *winner's* live
    /// blobs scheduled for deletion, and the sweeper duly deletes them out from
    /// under a committed frame. No race window is required — a store that
    /// re-packs a frame some earlier delete already queued hits the same thing.
    ///
    /// A token per attempt makes every attempt's blobs its own, so a rollback
    /// can only ever condemn blobs nobody else is referencing.
    ///
    /// The content hash stays: it costs nothing and it is what makes a blob's
    /// contents self-describing. It was never buying deduplication — the prefix
    /// already scoped keys to one frame.
    ///
    /// The cost is that a failed attempt now leaks a full set of blobs instead
    /// of being overwritten in place by the retry, so it leans harder on the
    /// sweeper actually working.
    pub fn pack_config_for_key(&self, key: &GraphTimeKey) -> ArrayGraphSerializablePackageConfig {
        let mut config = self.base_pack_config.clone();
        let timeline_id = key.timeline_id.clone();
        let graph_id = key.graph_id;
        let attempt = new_attempt_token();
        config.modify_blob_id = Some(Arc::new(move |id| {
            BlobID(format!(
                "graphs/{}/{}/{}/{}",
                timeline_id.0, graph_id.0, attempt, id
            ))
        }));
        config
    }
}

/// A token identifying one store attempt, unique across processes and hosts.
///
/// 64 random bits. The only requirement is that two attempts never agree, and
/// they cannot coordinate — they may be on different hosts, in jobs that know
/// nothing about each other — so randomness is the only thing that works. A
/// counter or a timestamp would collide exactly when two writers start
/// together, which is the case this exists for.
fn new_attempt_token() -> String {
    format!("{:016x}", rand::rng().random::<u64>())
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Shared database context — provides storage and pack configuration
//! to all namespace handles.

use std::sync::Arc;

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
    /// Clones the base config and sets `modify_blob_id` to prefix all
    /// blob IDs with `graphs/{timeline_id}/{graph_id}/`. This ensures
    /// each frame's blobs have unique IDs and can be independently
    /// deleted when the frame is removed.
    pub fn pack_config_for_key(&self, key: &GraphTimeKey) -> ArrayGraphSerializablePackageConfig {
        let mut config = self.base_pack_config.clone();
        let timeline_id = key.timeline_id.clone();
        let graph_id = key.graph_id;
        config.modify_blob_id = Some(Arc::new(move |id| {
            BlobID(format!("graphs/{}/{}/{}", timeline_id.0, graph_id.0, id))
        }));
        config
    }
}

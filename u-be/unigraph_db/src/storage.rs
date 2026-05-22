// Copyright (c) Meta Platforms, Inc. and affiliates.

//! The [`UnigraphStorage`] compositor — high-level graph store/fetch operations.
//!
//! Combines a [`UnigraphGraphStorage`] (frame metadata + inline blobs) with a
//! [`UnigraphBlobStorage`] (external blob storage) to provide full graph
//! lifecycle management: store full graphs, store deltas, store errors,
//! and reconstruct graphs from delta chains.

use std::sync::Arc;

use unigraph_storage_core::UnigraphBlobStorage;
use unigraph_storage_core::UnigraphGraphStorage;

/// High-level storage compositor that provides graph store/fetch operations.
///
/// Delegates low-level persistence to a [`UnigraphGraphStorage`] (frames table)
/// and a [`UnigraphBlobStorage`] (external blob store). Handles the decision
/// of whether to inline blobs or store them externally based on
/// [`INLINE_BLOB_THRESHOLD_BYTES`](unigraph_storage_core::INLINE_BLOB_THRESHOLD_BYTES).
pub struct UnigraphStorage {
    pub graph: Arc<dyn UnigraphGraphStorage>,
    pub blob: Arc<dyn UnigraphBlobStorage>,
}

impl UnigraphStorage {
    pub fn new(graph: Arc<dyn UnigraphGraphStorage>, blob: Arc<dyn UnigraphBlobStorage>) -> Self {
        Self { graph, blob }
    }
}

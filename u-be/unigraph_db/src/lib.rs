// Copyright (c) Meta Platforms, Inc. and affiliates.

//! High-level graph database for Unigraph.
//!
//! [`UnigraphDb`] is the single entry point for all storage operations.
//! It wraps an [`UnigraphStorage`] compositor internally (which combines a
//! [`UnigraphGraphStorage`](unigraph_storage_core::UnigraphGraphStorage) backend
//! with a [`UnigraphBlobStorage`](unigraph_storage_core::UnigraphBlobStorage)
//! backend) and provides a namespaced API for graph lifecycle management.
//!
//! ```text
//! db.timelines.create(...)                    // timeline CRUD
//! db.frames.select(...)                       // frame queries
//! db.external_ids.add_new(...)                // external ID mapping
//! db.graph.fetch(...)                         // schema-dispatched graph operations
//! db.graph.adjacent_deltas.store_range(...)   // batch range operations
//! db.metric_history.fetch(...)                // metric history
//! db.blob_storage.sweep(...)                  // blob lifecycle
//! ```
//!
//! `UnigraphDb` is `Clone` (via `Arc`) and can be passed freely across threads.

pub(crate) mod context;
mod frame_storage;
pub mod graph_range;
pub mod metric_history;
mod namespaces;
pub(crate) mod schemas;
mod storage;

use std::sync::Arc;

use anyhow::Result;
use context::UnigraphDbContext;
pub use graph_range::GraphRange;
pub use graph_range::GraphRangeBuilder;
pub use namespaces::AdjacentDeltasOps;
pub use namespaces::BlobStorageOps;
pub use namespaces::ExternalIds;
pub use namespaces::Frames;
pub use namespaces::Graph;
pub use namespaces::MetricHistory;
pub use namespaces::Timelines;
pub use storage::UnigraphStorage;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_storage_core::UnigraphBlobStorage;
use unigraph_storage_core::UnigraphGraphConnection;
use unigraph_storage_core::UnigraphGraphStorage;

/// High-level graph database — the single entry point for all storage operations.
///
/// Provides namespaced access to different areas of the database through
/// public fields. `Clone` via internal `Arc`, safe to share across threads.
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
///
/// use unigraph_db::UnigraphDb;
///
/// // Assuming you have graph and blob storage implementations:
/// // let graph: Arc<dyn UnigraphGraphStorage> = ...;
/// // let blob: Arc<dyn UnigraphBlobStorage> = ...;
/// // let db = UnigraphDb::new(graph, blob);
/// //
/// // db.timelines.create(&timeline_id, &config).await?;
/// // db.graph.fetch(&key).await?;
/// // db.frames.list(&timeline_id).await?;
/// ```
#[derive(Clone)]
pub struct UnigraphDb {
    pub timelines: Timelines,
    pub frames: Frames,
    pub external_ids: ExternalIds,
    pub graph: Graph,
    pub metric_history: MetricHistory,
    pub blob_storage: BlobStorageOps,
    ctx: UnigraphDbContext,
}

impl UnigraphDb {
    /// Create a new `UnigraphDb` from graph and blob storage backends.
    pub fn new(graph: Arc<dyn UnigraphGraphStorage>, blob: Arc<dyn UnigraphBlobStorage>) -> Self {
        let storage = Arc::new(UnigraphStorage::new(graph, blob));
        let ctx = UnigraphDbContext {
            storage,
            base_pack_config: ArrayGraphSerializablePackageConfig::default(),
        };
        Self {
            timelines: Timelines { ctx: ctx.clone() },
            frames: Frames { ctx: ctx.clone() },
            external_ids: ExternalIds { ctx: ctx.clone() },
            graph: Graph {
                ctx: ctx.clone(),
                adjacent_deltas: AdjacentDeltasOps { ctx: ctx.clone() },
            },
            metric_history: MetricHistory { ctx: ctx.clone() },
            blob_storage: BlobStorageOps { ctx: ctx.clone() },
            ctx,
        }
    }

    /// Get a raw graph connection for manual transaction control.
    ///
    /// Most callers should use the namespaced methods instead.
    /// Use this only when you need to hold a transaction across multiple operations
    /// (e.g. the ingestion pipeline's registration phase).
    pub async fn graph_conn(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.ctx.storage.graph.conn().await
    }

    /// Get a raw write connection for manual transaction control.
    ///
    /// Like [`graph_conn`](Self::graph_conn), but routes to the primary (read-write)
    /// connection pool. Use this when you need to perform writes outside the
    /// namespaced API.
    pub async fn graph_conn_write(&self) -> Result<Box<dyn UnigraphGraphConnection + '_>> {
        self.ctx.storage.graph.conn_write().await
    }
}

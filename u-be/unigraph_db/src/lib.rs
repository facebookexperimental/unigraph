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
//! db.graph_history.ingest(...)                // plain-row metric history
//! db.blob_storage.sweep(...)                  // blob lifecycle
//! ```
//!
//! `UnigraphDb` is `Clone` (via `Arc`) and can be passed freely across threads.

pub(crate) mod config_storage;
pub(crate) mod context;
mod frame_storage;
pub mod graph_history;
pub mod graph_range;
pub mod metric_history;
mod namespaces;
mod resolve;
pub(crate) mod schemas;
mod storage;

use std::sync::Arc;

use anyhow::Result;
pub use context::DEFAULT_CONFIG_INLINE_BLOB_THRESHOLD;
use context::UnigraphDbContext;
pub use graph_range::GraphRange;
pub use graph_range::GraphRangeBuilder;
pub use namespaces::AdjacentDeltasOps;
pub use namespaces::BlobStorageOps;
pub use namespaces::CleanupResult;
pub use namespaces::Configs;
pub use namespaces::ExternalIds;
pub use namespaces::Frames;
pub use namespaces::Graph;
pub use namespaces::GraphHistory;
pub use namespaces::HistoryCompactOptions;
pub use namespaces::HistoryCompactReport;
pub use namespaces::HistoryDeleteReport;
pub use namespaces::HistoryIngestOptions;
pub use namespaces::HistoryIngestReport;
pub use namespaces::HistorySeriesRow;
pub use namespaces::MetricHistory;
pub use namespaces::Timelines;
pub use namespaces::Utility;
pub use storage::UnigraphStorage;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::GraphHandle;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::UnigraphBlobStorage;
use unigraph_storage_core::UnigraphGraphConnection;
use unigraph_storage_core::UnigraphGraphStorage;

const MAX_GQC_RECURSION_DEPTH: usize = 5;

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
    pub configs: Configs,
    pub metric_history: MetricHistory,
    /// Decoupled, after-the-fact per-node metric history (`unigraph history`).
    pub graph_history: GraphHistory,
    pub blob_storage: BlobStorageOps,
    pub utility: Utility,
    ctx: UnigraphDbContext,
}

impl UnigraphDb {
    /// Create a new `UnigraphDb` from graph and blob storage backends.
    pub fn new(graph: Arc<dyn UnigraphGraphStorage>, blob: Arc<dyn UnigraphBlobStorage>) -> Self {
        let storage = Arc::new(UnigraphStorage::new(graph, blob));
        let ctx = UnigraphDbContext {
            storage,
            base_pack_config: ArrayGraphSerializablePackageConfig::default(),
            config_inline_blob_threshold: DEFAULT_CONFIG_INLINE_BLOB_THRESHOLD,
        };
        Self::from_ctx(ctx)
    }

    /// Set the threshold (in bytes) above which config blobs are stored in
    /// external blob storage instead of inline in the configs table.
    ///
    /// Default: [`DEFAULT_CONFIG_INLINE_BLOB_THRESHOLD`] (5 KB).
    pub fn with_config_inline_blob_threshold(mut self, threshold: usize) -> Self {
        self.ctx.config_inline_blob_threshold = threshold;
        // Rebuild namespace handles so they pick up the new threshold.
        Self::from_ctx(self.ctx)
    }

    fn from_ctx(ctx: UnigraphDbContext) -> Self {
        Self {
            timelines: Timelines { ctx: ctx.clone() },
            frames: Frames { ctx: ctx.clone() },
            external_ids: ExternalIds { ctx: ctx.clone() },
            graph: Graph {
                ctx: ctx.clone(),
                adjacent_deltas: AdjacentDeltasOps { ctx: ctx.clone() },
            },
            configs: Configs { ctx: ctx.clone() },
            metric_history: MetricHistory { ctx: ctx.clone() },
            graph_history: GraphHistory { ctx: ctx.clone() },
            blob_storage: BlobStorageOps { ctx: ctx.clone() },
            utility: Utility { ctx: ctx.clone() },
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

    /// Resolve a [`GraphHandle`] to a concrete graph, recursively following GQC keys.
    ///
    /// - `GraphKey` — fetches the specific snapshot.
    /// - `TimelineID` — fetches the latest graph.
    /// - `GqcKey` — fetches the GQC config, then resolves its inner handle
    ///   (which may itself be another GQC, up to [`MAX_GQC_RECURSION_DEPTH`]).
    pub async fn fetch_graph_by_handle(
        &self,
        handle: &GraphHandle,
        task: &ll::Task,
    ) -> Result<(GraphKey, ArrayGraphSerializable)> {
        self.fetch_graph_by_handle_recursive(handle, task, 0).await
    }
}

// -- Recursive handle resolution (private) ------------------------------------

impl UnigraphDb {
    #[expect(
        clippy::type_complexity,
        reason = "recursive async requires explicit Pin<Box<dyn Future>>"
    )]
    fn fetch_graph_by_handle_recursive<'a>(
        &'a self,
        handle: &'a GraphHandle,
        task: &'a ll::Task,
        depth: usize,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(GraphKey, ArrayGraphSerializable)>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            match handle {
                GraphHandle::GraphKey(key) => {
                    let ags = self.graph.fetch(key, task).await?;
                    Ok((key.clone(), ags))
                }
                GraphHandle::TimelineID(tid) => self.graph.fetch_latest(tid, task).await,
                GraphHandle::GqcKey(gqc_key) => {
                    anyhow::ensure!(
                        depth < MAX_GQC_RECURSION_DEPTH,
                        "GQC recursion depth exceeded (max {MAX_GQC_RECURSION_DEPTH})"
                    );
                    let gqc = self.configs.fetch_graph_query_config(gqc_key, task).await?;
                    self.fetch_graph_by_handle_recursive(&gqc.handle, task, depth + 1)
                        .await
                }
            }
        })
    }
}

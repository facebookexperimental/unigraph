// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Config storage — store and fetch content-addressed configs.
//!
//! Each config type has its own `store_*` / `fetch_*` method pair.
//! Configs are serialized as JSON, compressed with zstd, and stored under
//! content-addressed keys.
//!
//! Blobs below the inline threshold (see [`UnigraphDbContext::config_inline_blob_threshold`])
//! are stored directly in the configs SQL table. Larger blobs are offloaded to
//! external blob storage with crash-safe cleanup registration.
//!
//! `GraphQueryConfig` is stored in a composite form: the `TraversalConfig` is stored
//! separately (under its own content-addressed key) and referenced by key from the
//! stored `GraphQueryConfig`. This avoids re-storing the massive `TraversalConfig`
//! every time the tiny `GraphQueryConfig` changes.

use std::collections::BTreeSet;

use anyhow::Result;
use ll::task;
use serde::Deserialize;
use serde::Serialize;
use unigraph_core::TraversalConfig;
use unigraph_core::config_key::ConfigKeyLike;
use unigraph_core::config_key::ConfigRow;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::config_query::TraversalOverride;
use unigraph_core::types::NodeName;
use unigraph_storage_core::UnigraphGraphConnection;
use unigraph_timestamp::Timestamp;

use crate::config_storage::AsyncGetFn;
use crate::config_storage::AsyncStoreFn;
use crate::config_storage::PreparedConfigBlob;
use crate::config_storage::config_blob_path;
use crate::config_storage::fetch_config;
use crate::config_storage::prepare_config_blob;
use crate::config_storage::store_config;
use crate::context::UnigraphDbContext;

/// Storage form of GraphQueryConfig — references TVC by key instead of embedding it.
/// Private to unigraph_db — callers only see GraphQueryConfig.
#[derive(Serialize, Deserialize)]
struct GraphQueryConfigStored {
    handle: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    roots: Option<BTreeSet<NodeName>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    traversal_config_key: Option<TraversalConfigKey>,
}

/// Handle for config operations.
///
/// Obtained via [`UnigraphDb::configs`](crate::UnigraphDb).
#[derive(Clone)]
pub struct Configs {
    pub(crate) ctx: UnigraphDbContext,
}

impl Configs {
    // -- TraversalConfig --

    /// Store a traversal config.
    ///
    /// Returns the content-addressed key for the stored config.
    #[task(tags(l3))]
    pub async fn store_traversal_config(
        &self,
        config: &TraversalConfig,
        task: &ll::Task,
    ) -> Result<TraversalConfigKey> {
        let prepared = prepare_config_blob(config)?;
        self.store_prepared(prepared, StoreTraversalConfig, &task)
            .await
    }

    /// Fetch a traversal config by key.
    #[task(tags(l3))]
    pub async fn fetch_traversal_config(
        &self,
        key: &TraversalConfigKey,
        task: &ll::Task,
    ) -> Result<TraversalConfig> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        fetch_config(
            &mut *conn,
            key,
            GetTraversalConfig,
            &*self.ctx.storage.blob,
            &task,
        )
        .await
    }

    // -- GraphQueryConfig (composite) --

    /// Store a graph query config.
    ///
    /// If the traversal is `Inline`, the TVC is stored first under its own
    /// content-addressed key. If it's already a `Key`, that key is used directly.
    /// The GQC blob always stores only the TVC key reference.
    #[task(tags(l3))]
    pub async fn store_graph_query_config(
        &self,
        config: &GraphQueryConfig,
        task: &ll::Task,
    ) -> Result<GraphQueryConfigKey> {
        let traversal_config_key = match &config.traversal {
            Some(TraversalOverride::Inline(tvc)) => {
                Some(self.store_traversal_config(tvc, &task).await?)
            }
            Some(TraversalOverride::Key(key)) => Some(key.clone()),
            None => None,
        };

        let stored = GraphQueryConfigStored {
            handle: config.handle.to_string(),
            roots: config.roots.clone(),
            traversal_config_key,
        };

        let prepared = prepare_config_blob(&stored)?;
        self.store_prepared(prepared, StoreGraphQueryConfig, &task)
            .await
    }

    /// Fetch a graph query config by key.
    ///
    /// Returns the traversal as `TraversalOverride::Key` (lazy — the caller
    /// resolves the full TVC when needed).
    #[task(tags(l3))]
    pub async fn fetch_graph_query_config(
        &self,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<GraphQueryConfig> {
        let stored: GraphQueryConfigStored = {
            let mut conn = self.ctx.storage.graph.conn().await?;
            fetch_config(
                &mut *conn,
                key,
                GetGraphQueryConfig,
                &*self.ctx.storage.blob,
                &task,
            )
            .await?
        };

        let handle = stored
            .handle
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid handle in stored GQC: {e}"))?;

        let traversal = stored.traversal_config_key.map(TraversalOverride::Key);

        Ok(GraphQueryConfig {
            handle,
            roots: stored.roots,
            traversal,
        })
    }
}

// -- Private storage orchestration --

impl Configs {
    /// Store a prepared config blob, choosing inline vs external based on threshold.
    ///
    /// For external blobs, follows the crash-safe pattern:
    /// 1. Register blob key for cleanup (separate conn, committed immediately)
    /// 2. Upload blob to external storage
    /// 3. Start transaction → store config row → unregister cleanup → commit
    async fn store_prepared<K>(
        &self,
        prepared: PreparedConfigBlob<K>,
        store_fn: impl AsyncStoreFn<K>,
        task: &ll::Task,
    ) -> Result<K>
    where
        K: ConfigKeyLike,
    {
        let threshold = self.ctx.config_inline_blob_threshold;

        if prepared.blob.len() > threshold {
            self.store_external(prepared, store_fn, task).await
        } else {
            self.store_inline(prepared, store_fn, task).await
        }
    }

    /// Store a config with the blob inline in the SQL row.
    async fn store_inline<K>(
        &self,
        prepared: PreparedConfigBlob<K>,
        store_fn: impl AsyncStoreFn<K>,
        task: &ll::Task,
    ) -> Result<K>
    where
        K: ConfigKeyLike,
    {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        store_config(
            &mut *conn,
            &prepared.key,
            Some(&prepared.blob),
            None,
            store_fn,
            None,
            task,
        )
        .await?;
        Ok(prepared.key)
    }

    /// Store a config with the blob in external storage (crash-safe).
    async fn store_external<K>(
        &self,
        prepared: PreparedConfigBlob<K>,
        store_fn: impl AsyncStoreFn<K>,
        task: &ll::Task,
    ) -> Result<K>
    where
        K: ConfigKeyLike,
    {
        let blob_path = config_blob_path(&prepared.key);

        // 1. Register for cleanup (separate conn — persists on crash)
        let mut reg_conn = self.ctx.storage.graph.conn_write().await?;
        reg_conn
            .register_blobs_for_cleanup(std::slice::from_ref(&blob_path), task)
            .await?;
        drop(reg_conn);

        // 2. Upload blob to external storage (outside any transaction)
        self.ctx
            .storage
            .blob
            .put_blob(&blob_path, &prepared.blob)
            .await?;

        // 3. Transaction: store config row + unregister cleanup
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        store_config(
            &mut *conn,
            &prepared.key,
            None,
            Some(&blob_path),
            store_fn,
            None,
            task,
        )
        .await?;
        conn.unregister_blobs_for_cleanup(&[blob_path], task)
            .await?;
        conn.commit_transaction(task).await?;

        Ok(prepared.key)
    }
}

// -- AsyncStoreFn / AsyncGetFn implementations --

struct StoreTraversalConfig;

#[async_trait::async_trait]
impl AsyncStoreFn<TraversalConfigKey> for StoreTraversalConfig {
    async fn call(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &TraversalConfigKey,
        blob_inline: Option<&[u8]>,
        blob_id: Option<&str>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()> {
        conn.store_traversal_config(key, blob_inline, blob_id, expires_at, task)
            .await
    }
}

struct GetTraversalConfig;

#[async_trait::async_trait]
impl AsyncGetFn<TraversalConfigKey> for GetTraversalConfig {
    async fn call(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &TraversalConfigKey,
        task: &ll::Task,
    ) -> Result<Option<ConfigRow<TraversalConfigKey>>> {
        conn.get_traversal_config(key, task).await
    }
}

struct StoreGraphQueryConfig;

#[async_trait::async_trait]
impl AsyncStoreFn<GraphQueryConfigKey> for StoreGraphQueryConfig {
    async fn call(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &GraphQueryConfigKey,
        blob_inline: Option<&[u8]>,
        blob_id: Option<&str>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()> {
        conn.store_graph_query_config(key, blob_inline, blob_id, expires_at, task)
            .await
    }
}

struct GetGraphQueryConfig;

#[async_trait::async_trait]
impl AsyncGetFn<GraphQueryConfigKey> for GetGraphQueryConfig {
    async fn call(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &GraphQueryConfigKey,
        task: &ll::Task,
    ) -> Result<Option<ConfigRow<GraphQueryConfigKey>>> {
        conn.get_graph_query_config(key, task).await
    }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Config storage — store and fetch content-addressed configs.
//!
//! Each config type has its own `store_*` / `fetch_*` method pair.
//! Configs are serialized as JSON, compressed with zstd, and stored under
//! content-addressed keys.
//!
//! Blobs below the inline threshold (see [`UnigraphDbContext::config_inline_blob_threshold`])
//! are stored directly in the configs SQL table. Larger blobs are offloaded to
//! external blob storage.
//!
//! `GraphQueryConfig` is stored in a composite form: the `TraversalConfig` is stored
//! separately (under its own content-addressed key) and referenced by key from the
//! stored `GraphQueryConfig`. This avoids re-storing the massive `TraversalConfig`
//! every time the tiny `GraphQueryConfig` changes.
//!
//! # Storing is a batch operation
//!
//! Everything goes through [`Configs::store_batch`], single stores included.
//! A batch is four steps, and each one is the reason the previous shape didn't
//! work:
//!
//! ```text
//! prepare   serialize + compress + hash, in parallel   -> one key per input
//! narrow    dedup by key, then one SELECT for the rest -> only genuinely new configs
//! upload    put every external blob, concurrently      -> no DB involvement
//! commit    one transaction, one INSERT IGNORE         -> rows for the batch
//! ```
//!
//! **Narrowing is what keeps the writes off the database.** Keys are hashes of
//! the compressed bytes, so a key that is already stored is a config whose row
//! and blob are already exactly right. The WWW graph build stores ~50 per-project
//! traversal configs on every run and they are overwhelmingly the same configs as
//! last run — and often as each other, which is why the dedup happens before the
//! `SELECT` rather than after.
//!
//! **Blobs are not registered for cleanup, on purpose.** The crash-safe dance the
//! frame path uses — register the key, upload, unregister on commit — is unsound
//! here, because a config blob path is `configs/{prefix}/{content hash}` with no
//! random suffix. Two jobs storing the same config produce the *same* key, so a
//! loser's failed store leaves the winner's live blob queued for the sweeper.
//! That is the mechanism that cost the `www` timeline 664 frames before graph blob
//! keys grew a random suffix. The trade is a blob orphaned whenever a store dies
//! between upload and commit; nothing ever collects those, but they are small,
//! rare, and vastly cheaper than deleting a config someone is using.
//!
//! **One transaction, rows sorted by key.** Concurrent batches that overlap take
//! their row locks in the same order, so they queue instead of deadlocking. The
//! sort is free — the dedup map is already ordered.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
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
use unigraph_storage_core::ConfigWrite;
use unigraph_storage_core::UnigraphGraphConnection;

use crate::config_storage::AsyncGetFn;
use crate::config_storage::PreparedConfigBlob;
use crate::config_storage::config_blob_path;
use crate::config_storage::fetch_config;
use crate::config_storage::prepare_config_blob;
use crate::config_storage::prepare_config_blobs;
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
        let mut keys = self.store_traversal_configs(&[config], &task).await?;
        keys.pop()
            .context("store_traversal_configs returned no key for one config")
    }

    /// Store several traversal configs as one batch.
    ///
    /// Returns one key per input, in input order. Duplicates in `configs` are
    /// collapsed to a single write and get the same key back — see the module
    /// docs for what the batch actually does.
    ///
    /// Takes references rather than owned configs: a `TraversalConfig` can be
    /// megabytes, and the caller almost always already has them in hand.
    #[task(tags(l3))]
    pub async fn store_traversal_configs(
        &self,
        configs: &[&TraversalConfig],
        task: &ll::Task,
    ) -> Result<Vec<TraversalConfigKey>> {
        let prepared = prepare_config_blobs(configs)?;
        self.store_batch(prepared, &task).await
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
        let mut keys = self.store_batch(vec![prepared], &task).await?;
        keys.pop()
            .context("store_batch returned no key for one config")
    }

    /// Resolve a `TraversalOverride` to a full `TraversalConfig`.
    ///
    /// `Inline` configs are returned as-is. `Key` references are fetched from storage.
    pub async fn resolve_traversal_override(
        &self,
        traversal: &TraversalOverride,
        task: &ll::Task,
    ) -> Result<TraversalConfig> {
        match traversal {
            TraversalOverride::Inline(tc) => Ok(tc.clone()),
            TraversalOverride::Key(key) => self.fetch_traversal_config(key, task).await,
        }
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
    /// Store a batch of prepared configs. Returns one key per input, in order.
    ///
    /// See the module docs for why it is shaped this way.
    async fn store_batch<K>(
        &self,
        prepared: Vec<PreparedConfigBlob<K>>,
        task: &ll::Task,
    ) -> Result<Vec<K>>
    where
        K: ConfigKeyLike + Send + Sync,
    {
        if prepared.is_empty() {
            return Ok(Vec::new());
        }

        let keys: Vec<K> = prepared.iter().map(|p| p.key.clone()).collect();
        task.data("requested", keys.len());

        let unwritten = self.narrow_to_unwritten(&prepared, task).await?;
        task.data("to_write", unwritten.len());
        if unwritten.is_empty() {
            return Ok(keys);
        }

        let rows = self.upload_blobs_and_build_rows(&unwritten, task).await?;
        self.commit_rows(&rows, task).await?;

        Ok(keys)
    }

    /// Drop the configs that need no write: duplicates within the batch, and
    /// keys already in the database.
    ///
    /// Returns them ordered by key — that ordering is what
    /// [`Self::commit_rows`] relies on to keep concurrent batches from
    /// deadlocking, and a `BTreeMap` hands it over for free.
    ///
    /// The existence check reads from a replica. Stale data can only say "not
    /// there" about something that is, which costs a redundant `INSERT IGNORE`
    /// — the same write this path was already doing unconditionally.
    async fn narrow_to_unwritten<'a, K>(
        &self,
        prepared: &'a [PreparedConfigBlob<K>],
        task: &ll::Task,
    ) -> Result<Vec<&'a PreparedConfigBlob<K>>>
    where
        K: ConfigKeyLike + Send + Sync,
    {
        let unique: BTreeMap<&str, &PreparedConfigBlob<K>> =
            prepared.iter().map(|p| (p.key.as_str(), p)).collect();

        let unique_keys: Vec<String> = unique.keys().map(|k| (*k).to_owned()).collect();
        let stored = {
            let mut conn = self.ctx.storage.graph.conn().await?;
            conn.select_stored_config_keys(&unique_keys, task).await?
        };

        Ok(unique
            .into_values()
            .filter(|p| !stored.contains(p.key.as_str()))
            .collect())
    }

    /// Upload every external blob, then describe all the rows to write.
    ///
    /// Uploads happen before any transaction is open and are not registered for
    /// cleanup — see the module docs. A config under the inline threshold has no
    /// blob to upload; its bytes ride along in the row.
    async fn upload_blobs_and_build_rows<K>(
        &self,
        unwritten: &[&PreparedConfigBlob<K>],
        task: &ll::Task,
    ) -> Result<Vec<ConfigWrite>>
    where
        K: ConfigKeyLike,
    {
        let threshold = self.ctx.config_inline_blob_threshold;
        let mut external: Vec<&PreparedConfigBlob<K>> = Vec::new();
        let mut rows: Vec<ConfigWrite> = Vec::with_capacity(unwritten.len());

        for prepared in unwritten {
            if prepared.blob.len() > threshold {
                external.push(prepared);
            } else {
                rows.push(config_write::<K>(prepared, None));
            }
        }
        task.data("external", external.len());
        task.data("inline", rows.len());

        let uploads = external.iter().map(|prepared| async move {
            let path = config_blob_path(&prepared.key);
            self.ctx
                .storage
                .blob
                .put_blob(&path, &prepared.blob)
                .await
                .with_context(|| format!("failed to upload config blob: {path}"))?;
            Ok::<_, anyhow::Error>(config_write::<K>(prepared, Some(path)))
        });
        rows.extend(futures::future::try_join_all(uploads).await?);

        // Inline and external rows were accumulated separately and the uploads
        // finished out of order, so restore the key ordering the write wants.
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(rows)
    }

    /// Write the rows in one transaction.
    ///
    /// `rows` is sorted by key, so two batches that overlap take their locks in
    /// the same order and queue behind each other rather than deadlocking.
    async fn commit_rows(&self, rows: &[ConfigWrite], task: &ll::Task) -> Result<()> {
        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.store_configs(rows, task).await?;
        conn.commit_transaction(task).await
    }
}

/// Describe one row. `blob_path` is `Some` for an external blob, `None` to
/// carry the compressed bytes inline in the row.
fn config_write<K: ConfigKeyLike>(
    prepared: &PreparedConfigBlob<K>,
    blob_path: Option<String>,
) -> ConfigWrite {
    ConfigWrite {
        key: prepared.key.as_str().to_owned(),
        config_type: K::PREFIX.to_owned(),
        blob_inline: blob_path.is_none().then(|| prepared.blob.clone()),
        blob_id: blob_path,
        expires_at: None,
    }
}

// -- AsyncGetFn implementations --

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

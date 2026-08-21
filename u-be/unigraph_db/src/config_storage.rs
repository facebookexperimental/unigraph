// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Config storage — store and fetch content-addressed configs.
//!
//! Each config type (TraversalConfig, GraphQueryConfig) uses the same
//! store/fetch pattern: serialize as JSON, compress with zstd, compute a
//! content-addressed key from the blob.
//!
//! Blobs below the inline threshold are stored directly in the configs SQL table.
//! Larger blobs are offloaded to external blob storage under `configs/{PREFIX}/{key}`.

use anyhow::Context;
use anyhow::Result;
use unigraph_core::config_key::ConfigKeyLike;
use unigraph_core::config_key::ConfigRow;
use unigraph_serialization::ZSTDCompressionLevel;
use unigraph_serialization::from_zstd;
use unigraph_serialization::to_zstd;
use unigraph_storage_core::UnigraphBlobStorage;
use unigraph_storage_core::UnigraphGraphConnection;
use unigraph_timestamp::Timestamp;

// -- Blob preparation --

/// A config blob ready for storage, with the content-addressed key already computed.
pub(crate) struct PreparedConfigBlob<K> {
    pub key: K,
    pub blob: Vec<u8>,
}

/// Serialize a config value, compress, and compute the content-addressed key.
pub(crate) fn prepare_config_blob<T, K>(value: &T) -> Result<PreparedConfigBlob<K>>
where
    T: serde::Serialize,
    K: ConfigKeyLike,
{
    let json = serde_json::to_vec(value).context("failed to serialize config")?;
    let blob = to_zstd(&json, ZSTDCompressionLevel::Best).context("failed to compress config")?;
    let key = K::from_blob(&blob);
    Ok(PreparedConfigBlob { key, blob })
}

/// External blob storage path for a config key: `configs/{PREFIX}/{key}`.
pub(crate) fn config_blob_path<K: ConfigKeyLike>(key: &K) -> String {
    format!("configs/{}/{}", K::PREFIX, key)
}

// -- Blob resolution --

/// Resolve the compressed blob bytes from a config row.
///
/// Checks `blob_inline` first; if absent, fetches from external blob storage
/// using `blob_id`.
async fn resolve_config_blob<K: ConfigKeyLike>(
    row: &ConfigRow<K>,
    blob_storage: &(dyn UnigraphBlobStorage + Send + Sync),
) -> Result<Vec<u8>> {
    if let Some(ref inline) = row.blob_inline {
        return Ok(inline.clone());
    }

    let blob_id = row
        .blob_id
        .as_ref()
        .context("config has neither inline blob nor blob_id")?;

    blob_storage
        .get_blob(blob_id)
        .await
        .with_context(|| format!("failed to read config blob from external storage: {blob_id}"))
}

// -- Store --

/// Write a config row to the database. The caller decides inline vs external
/// and passes the appropriate combination of `blob_inline` / `blob_id`.
pub(crate) async fn store_config<K>(
    conn: &mut dyn UnigraphGraphConnection,
    key: &K,
    blob_inline: Option<&[u8]>,
    blob_id: Option<&str>,
    store_fn: impl AsyncStoreFn<K>,
    expires_at: Option<Timestamp>,
    task: &ll::Task,
) -> Result<()>
where
    K: ConfigKeyLike,
{
    store_fn
        .call(conn, key, blob_inline, blob_id, expires_at, task)
        .await
}

// -- Fetch --

/// Fetch a config value by key, deserializing from the stored blob.
pub(crate) async fn fetch_config<T, K>(
    conn: &mut dyn UnigraphGraphConnection,
    key: &K,
    get_fn: impl AsyncGetFn<K>,
    blob_storage: &(dyn UnigraphBlobStorage + Send + Sync),
    task: &ll::Task,
) -> Result<T>
where
    T: serde::de::DeserializeOwned,
    K: ConfigKeyLike + Send + Sync,
{
    let row = get_fn
        .call(conn, key, task)
        .await?
        .with_context(|| format!("config not found: {}", key))?;

    let blob = resolve_config_blob(&row, blob_storage).await?;
    let json = from_zstd(&blob).context("failed to decompress config")?;
    let value: T = serde_json::from_slice(&json).context("failed to parse config JSON")?;
    Ok(value)
}

// -- Async function traits for passing trait methods as callbacks --

/// Trait for async store functions (wraps the typed trait methods).
#[async_trait::async_trait]
pub(crate) trait AsyncStoreFn<K: ConfigKeyLike>: Send + Sync {
    async fn call(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &K,
        blob_inline: Option<&[u8]>,
        blob_id: Option<&str>,
        expires_at: Option<Timestamp>,
        task: &ll::Task,
    ) -> Result<()>;
}

/// Trait for async get functions (wraps the typed trait methods).
#[async_trait::async_trait]
pub(crate) trait AsyncGetFn<K: ConfigKeyLike>: Send + Sync {
    async fn call(
        &self,
        conn: &mut dyn UnigraphGraphConnection,
        key: &K,
        task: &ll::Task,
    ) -> Result<Option<ConfigRow<K>>>;
}

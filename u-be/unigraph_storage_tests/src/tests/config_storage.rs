// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
use unigraph_core::Decision;
use unigraph_core::TraversalConfig;
use unigraph_core::config_key::ConfigKeyLike;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::UnigraphBlobStorage;
use unigraph_storage_sqlite::SqliteStorage;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

fn sample_traversal_config() -> TraversalConfig {
    let mut force_nodes = BTreeMap::new();
    force_nodes.insert("moduleA".to_string(), Decision::include());
    force_nodes.insert("moduleB".to_string(), Decision::exclude());
    TraversalConfig {
        force_nodes: Some(force_nodes),
        force_edges: None,
        force_tagged: None,
        label_predicates: None,
        force_dynamic: None,
        tiered_traversal: None,
        messages: None,
    }
}

fn sample_graph_query_config() -> GraphQueryConfig {
    GraphQueryConfig {
        handle: "my_timeline~42".parse().unwrap(),
        roots: Some(BTreeSet::from(["root1".to_string(), "root2".to_string()])),
        traversal: Some(unigraph_core::config_query::TraversalOverride::Inline(
            sample_traversal_config(),
        )),
    }
}

// -- TraversalConfig tests --

#[tokio::test]
async fn traversal_config_store_and_fetch() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = sample_traversal_config();
    let key = db.configs.store_traversal_config(&config, &task).await?;

    assert!(key.as_str().starts_with("tvc_"));

    let fetched = db.configs.fetch_traversal_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

#[tokio::test]
async fn traversal_config_deduplication() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = sample_traversal_config();
    let key1 = db.configs.store_traversal_config(&config, &task).await?;
    let key2 = db.configs.store_traversal_config(&config, &task).await?;

    // Same config produces same key
    assert_eq!(key1, key2);

    Ok(())
}

// -- GraphQueryConfig tests (composite storage) --

#[tokio::test]
async fn graph_query_config_store_and_fetch() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = sample_graph_query_config();
    let key = db.configs.store_graph_query_config(&config, &task).await?;

    assert!(key.as_str().starts_with("gqc_"));

    let fetched = db.configs.fetch_graph_query_config(&key, &task).await?;
    // Fetched GQC has TraversalOverride::Key (lazy) instead of Inline
    assert_eq!(fetched.handle, config.handle);
    assert_eq!(fetched.roots, config.roots);
    assert!(matches!(
        fetched.traversal,
        Some(unigraph_core::config_query::TraversalOverride::Key(_))
    ));

    Ok(())
}

#[tokio::test]
async fn graph_query_config_without_traversal() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = GraphQueryConfig {
        handle: "only_root_timeline".parse().unwrap(),
        roots: Some(BTreeSet::from(["only_root".to_string()])),
        traversal: None,
    };

    let key = db.configs.store_graph_query_config(&config, &task).await?;
    let fetched = db.configs.fetch_graph_query_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

#[tokio::test]
async fn graph_query_config_tvc_dedup() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    // Two GQCs with different roots but the same TVC
    let tvc = sample_traversal_config();

    let gqc1 = GraphQueryConfig {
        handle: "timeline_a".parse().unwrap(),
        roots: Some(BTreeSet::from(["root_a".to_string()])),
        traversal: Some(unigraph_core::config_query::TraversalOverride::Inline(
            tvc.clone(),
        )),
    };
    let gqc2 = GraphQueryConfig {
        handle: "timeline_a".parse().unwrap(),
        roots: Some(BTreeSet::from(["root_b".to_string()])),
        traversal: Some(unigraph_core::config_query::TraversalOverride::Inline(
            tvc.clone(),
        )),
    };

    let key1 = db.configs.store_graph_query_config(&gqc1, &task).await?;
    let key2 = db.configs.store_graph_query_config(&gqc2, &task).await?;

    // Different GQC keys (different roots)
    assert_ne!(key1, key2);

    // Both fetch back with same TVC key reference
    let fetched1 = db.configs.fetch_graph_query_config(&key1, &task).await?;
    let fetched2 = db.configs.fetch_graph_query_config(&key2, &task).await?;
    assert_eq!(fetched1.traversal, fetched2.traversal);

    Ok(())
}

#[tokio::test]
async fn graph_query_config_with_handle() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = GraphQueryConfig {
        handle: "my_timeline~123".parse().unwrap(),
        roots: None,
        traversal: None,
    };

    let key = db.configs.store_graph_query_config(&config, &task).await?;
    let fetched = db.configs.fetch_graph_query_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

// -- Key prefix verification --

#[tokio::test]
async fn key_prefixes_are_distinct() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let tvc_key = db
        .configs
        .store_traversal_config(&sample_traversal_config(), &task)
        .await?;
    let gqc_key = db
        .configs
        .store_graph_query_config(&sample_graph_query_config(), &task)
        .await?;

    assert!(tvc_key.as_str().starts_with("tvc_"));
    assert!(gqc_key.as_str().starts_with("gqc_"));

    assert!(tvc_key.as_str().starts_with(TraversalConfigKey::PREFIX));
    assert!(gqc_key.as_str().starts_with(GraphQueryConfigKey::PREFIX));

    Ok(())
}

// -- External blob storage --

#[tokio::test]
async fn large_config_goes_to_external_blob_storage() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone()).with_config_inline_blob_threshold(50);
    let task = ll::Task::create_new("test");

    let config = sample_traversal_config();
    let key = db.configs.store_traversal_config(&config, &task).await?;

    // Verify blob exists in external storage
    let blob_path = format!("configs/{}/{}", TraversalConfigKey::PREFIX, key);
    let blob = sqlite.get_blob(&blob_path).await?;
    assert!(blob.is_some(), "blob should exist in external storage");

    // Fetch resolves correctly
    let fetched = db.configs.fetch_traversal_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

#[tokio::test]
async fn small_config_stays_inline() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    let task = ll::Task::create_new("test");

    let config = sample_traversal_config();
    let key = db.configs.store_traversal_config(&config, &task).await?;

    // Verify no blob in external storage
    let blob_path = format!("configs/{}/{}", TraversalConfigKey::PREFIX, key);
    let blob = sqlite.get_blob(&blob_path).await?;
    assert!(blob.is_none(), "small config should be stored inline");

    // Fetch still works
    let fetched = db.configs.fetch_traversal_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

#[tokio::test]
async fn custom_threshold_forces_external_storage() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone()).with_config_inline_blob_threshold(0);
    let task = ll::Task::create_new("test");

    let config = sample_traversal_config();
    let key = db.configs.store_traversal_config(&config, &task).await?;

    // Even the small config should be in external storage with threshold=0
    let blob_path = format!("configs/{}/{}", TraversalConfigKey::PREFIX, key);
    let blob = sqlite.get_blob(&blob_path).await?;
    assert!(
        blob.is_some(),
        "with threshold=0, all blobs should be external"
    );

    let fetched = db.configs.fetch_traversal_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

// -- Not found --

#[tokio::test]
async fn fetch_nonexistent_config_errors() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let fake_key = TraversalConfigKey::from_string("tvc_0000000000000000".to_string())?;
    let err = db
        .configs
        .fetch_traversal_config(&fake_key, &task)
        .await
        .unwrap_err();
    let err_debug = format!("{err:?}");
    assert!(
        err_debug.contains("config not found"),
        "unexpected error: {err_debug}"
    );

    Ok(())
}

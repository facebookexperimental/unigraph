// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
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
    assert!(
        sqlite.has_blob(&blob_path).await?,
        "blob should exist in external storage"
    );

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
    assert!(
        !sqlite.has_blob(&blob_path).await?,
        "small config should be stored inline"
    );

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
    assert!(
        sqlite.has_blob(&blob_path).await?,
        "with threshold=0, all blobs should be external"
    );

    let fetched = db.configs.fetch_traversal_config(&key, &task).await?;
    assert_eq!(fetched, config);

    Ok(())
}

// -- Batch storage --

fn traversal_config_with(module: &str) -> TraversalConfig {
    let mut force_nodes = BTreeMap::new();
    force_nodes.insert(module.to_string(), Decision::include());
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

/// Keys come back one per input, in input order, and every one of them fetches
/// back the config that was at that position.
#[tokio::test]
async fn batch_returns_a_key_per_input_in_order() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let configs: Vec<TraversalConfig> = (0..5)
        .map(|i| traversal_config_with(&format!("module_{i}")))
        .collect();
    let refs: Vec<&TraversalConfig> = configs.iter().collect();

    let keys = db.configs.store_traversal_configs(&refs, &task).await?;

    assert_eq!(keys.len(), configs.len());
    for (key, config) in keys.iter().zip(&configs) {
        assert_eq!(
            &db.configs.fetch_traversal_config(key, &task).await?,
            config
        );
    }

    Ok(())
}

/// Repeats inside one batch collapse to a single write but still get their own
/// slot in the result. The WWW build hits this constantly — different budget
/// projects routinely produce identical patched TVCs.
#[tokio::test]
async fn batch_dedups_repeats_and_still_answers_every_slot() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let a = traversal_config_with("a");
    let b = traversal_config_with("b");
    let refs = vec![&a, &b, &a, &a, &b];

    let keys = db.configs.store_traversal_configs(&refs, &task).await?;

    assert_eq!(keys.len(), 5);
    assert_eq!(keys[0], keys[2], "same config, same key");
    assert_eq!(keys[0], keys[3]);
    assert_eq!(keys[1], keys[4]);
    assert_ne!(keys[0], keys[1], "different configs, different keys");

    assert_eq!(db.configs.fetch_traversal_config(&keys[0], &task).await?, a);
    assert_eq!(db.configs.fetch_traversal_config(&keys[1], &task).await?, b);

    Ok(())
}

/// A batch of configs that are all already stored writes nothing at all.
///
/// Deleting the blobs out from under the second batch is the only way to
/// observe from outside that it did no work — and that write amplification is
/// what put ~50 transactions a build onto the shared `configs` table.
#[tokio::test]
async fn batch_skips_configs_that_are_already_stored() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone()).with_config_inline_blob_threshold(0);
    let task = ll::Task::create_new("test");

    let configs: Vec<TraversalConfig> = (0..3)
        .map(|i| traversal_config_with(&format!("module_{i}")))
        .collect();
    let refs: Vec<&TraversalConfig> = configs.iter().collect();

    let keys = db.configs.store_traversal_configs(&refs, &task).await?;
    let paths: Vec<String> = keys
        .iter()
        .map(|key| format!("configs/{}/{}", TraversalConfigKey::PREFIX, key))
        .collect();
    for path in &paths {
        sqlite.delete_blob(path).await?;
    }

    let keys_again = db.configs.store_traversal_configs(&refs, &task).await?;

    assert_eq!(keys_again, keys, "content-addressed keys must be stable");
    for path in &paths {
        assert!(
            !sqlite.has_blob(path).await?,
            "re-store uploaded {path} again instead of skipping it"
        );
    }

    Ok(())
}

/// A batch of a mix — some stored, some new — writes only the new ones.
#[tokio::test]
async fn batch_writes_only_the_new_configs() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone()).with_config_inline_blob_threshold(0);
    let task = ll::Task::create_new("test");

    let old = traversal_config_with("old");
    let new = traversal_config_with("new");

    let old_key = db.configs.store_traversal_config(&old, &task).await?;
    let old_path = format!("configs/{}/{}", TraversalConfigKey::PREFIX, old_key);
    sqlite.delete_blob(&old_path).await?;

    let keys = db
        .configs
        .store_traversal_configs(&[&old, &new], &task)
        .await?;

    assert_eq!(keys[0], old_key);
    assert!(
        !sqlite.has_blob(&old_path).await?,
        "the already-stored config was rewritten"
    );

    let new_path = format!("configs/{}/{}", TraversalConfigKey::PREFIX, keys[1]);
    assert!(
        sqlite.has_blob(&new_path).await?,
        "the new config was not stored"
    );
    assert_eq!(
        db.configs.fetch_traversal_config(&keys[1], &task).await?,
        new
    );

    Ok(())
}

/// Storing a config never puts its blob on the sweeper's list.
///
/// Config blob paths are `configs/{prefix}/{content hash}` with no random
/// suffix, so every writer of a config registers the *same* key — registering
/// them at all is how a loser's failed store gets a winner's live blob deleted.
#[tokio::test]
async fn storing_configs_queues_nothing_for_cleanup() -> Result<()> {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone()).with_config_inline_blob_threshold(0);
    let task = ll::Task::create_new("test");

    let configs: Vec<TraversalConfig> = (0..3)
        .map(|i| traversal_config_with(&format!("module_{i}")))
        .collect();
    let refs: Vec<&TraversalConfig> = configs.iter().collect();

    let keys = db.configs.store_traversal_configs(&refs, &task).await?;

    assert_eq!(
        db.blob_storage.get_pending_cleanup(&task).await?,
        Vec::<String>::new(),
        "config blobs must never be registered for cleanup"
    );

    // A sweep with no deferral would have taken them if they had been.
    db.blob_storage
        .sweep(std::time::Duration::ZERO, None, &task)
        .await?;
    assert_eq!(
        db.configs.fetch_traversal_config(&keys[0], &task).await?,
        configs[0]
    );

    Ok(())
}

/// An empty batch does no work and returns nothing.
#[tokio::test]
async fn empty_batch_is_a_no_op() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let keys = db.configs.store_traversal_configs(&[], &task).await?;

    assert!(keys.is_empty());
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

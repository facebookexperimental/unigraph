// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;

use anyhow::Result;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

#[tokio::test]
async fn create_and_get_timeline_config() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };

    db.timelines
        .create(&TimelineID("my_timeline".to_string()), &config, &task)
        .await?;

    let fetched = db
        .timelines
        .get_config(&TimelineID("my_timeline".to_string()), &task)
        .await?
        .expect("Timeline should exist");

    // Verify the schema type survived round-trip
    match fetched.schema {
        TimelineSchema::AdjacentDeltas(_) => {} // expected
        _ => panic!("unexpected schema type"),
    }

    Ok(())
}

#[tokio::test]
async fn get_nonexistent_timeline_returns_none() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let result = db
        .timelines
        .get_config(&TimelineID("nonexistent".to_string()), &task)
        .await?;
    assert!(result.is_none());

    Ok(())
}

#[tokio::test]
async fn list_timelines() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };

    db.timelines
        .create(&TimelineID("beta".to_string()), &config, &task)
        .await?;
    db.timelines
        .create(&TimelineID("alpha".to_string()), &config, &task)
        .await?;
    db.timelines
        .create(&TimelineID("gamma".to_string()), &config, &task)
        .await?;

    let timelines = db.timelines.list(&task).await?;
    let names: Vec<_> = timelines.iter().map(|t| t.0.as_str()).collect();

    // Should be sorted
    assert_eq!(names, vec!["alpha", "beta", "gamma"]);

    Ok(())
}

#[tokio::test]
async fn frames_ordered_by_timestamp_then_graph_id() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };
    db.timelines
        .create(&TimelineID("test".to_string()), &config, &task)
        .await?;

    // Insert frames at the same timestamp with monotonically increasing graph IDs
    let ts = unigraph_timestamp::Timestamp::from_unix_timestamp(1000);

    for id in [1, 2, 3] {
        let graph = TestGraphTimeline::get_nth(id as u64);
        let key = GraphTimeKey {
            timeline_id: TimelineID("test".to_string()),
            timestamp: ts,
            graph_id: GraphID(id),
        };
        db.graph.store(&key, &graph, None, &task).await?;
    }

    let frames = db
        .frames
        .list(&TimelineID("test".to_string()), &task)
        .await?;
    let ids: Vec<_> = frames.iter().map(|f| f.frame.graph_id.0).collect();

    // Same timestamp → ordered by graph_id
    assert_eq!(ids, vec![1, 2, 3]);

    Ok(())
}

#[tokio::test]
async fn list_frames_range() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };
    db.timelines
        .create(&TimelineID("test".to_string()), &config, &task)
        .await?;

    for i in 0..10 {
        let graph = TestGraphTimeline::get_nth(i);
        let key = make_graph_time_key("test", i as i64, 1000 + i as i64);
        db.graph.store(&key, &graph, None, &task).await?;
    }

    // Query a range that should include frames 3-7
    let start = unigraph_timestamp::Timestamp::from_unix_timestamp(1003);
    let end = unigraph_timestamp::Timestamp::from_unix_timestamp(1007);

    let frames = db
        .frames
        .list_range(&TimelineID("test".to_string()), start, end, &task)
        .await?;

    let ids: Vec<_> = frames.iter().map(|f| f.frame.graph_id.0).collect();
    assert_eq!(ids, vec![3, 4, 5, 6, 7]);

    Ok(())
}

#[tokio::test]
async fn add_new_external_ids_allocates_sequentially() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let namespace = ExternalIDNamespace("test/git".to_string());
    let external_ids = vec![
        ExternalID("aaa".to_string()),
        ExternalID("bbb".to_string()),
        ExternalID("ccc".to_string()),
    ];

    let graph_ids = db
        .external_ids
        .add_new(&namespace, &external_ids, &task)
        .await?;

    assert_eq!(graph_ids.len(), 3);
    assert_eq!(graph_ids[0].0, 1);
    assert_eq!(graph_ids[1].0, 2);
    assert_eq!(graph_ids[2].0, 3);

    Ok(())
}

#[tokio::test]
async fn add_new_external_ids_is_idempotent() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let namespace = ExternalIDNamespace("test/git".to_string());
    let external_ids = vec![ExternalID("aaa".to_string()), ExternalID("bbb".to_string())];

    // First call: allocate
    let first = db
        .external_ids
        .add_new(&namespace, &external_ids, &task)
        .await?;

    // Second call: same inputs, same outputs
    let second = db
        .external_ids
        .add_new(&namespace, &external_ids, &task)
        .await?;

    assert_eq!(first, second);

    Ok(())
}

#[tokio::test]
async fn external_id_mapping_roundtrip() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let namespace = ExternalIDNamespace("test/git".to_string());
    let external_ids = vec![
        ExternalID("commit_abc".to_string()),
        ExternalID("commit_def".to_string()),
    ];

    // Allocate
    let graph_ids = db
        .external_ids
        .add_new(&namespace, &external_ids, &task)
        .await?;

    // Reverse lookup: graph_id → external_id
    let eid_0 = db
        .external_ids
        .to_external_id(&namespace, &graph_ids[0], &task)
        .await?;
    assert_eq!(eid_0, Some(ExternalID("commit_abc".to_string())));

    let eid_1 = db
        .external_ids
        .to_external_id(&namespace, &graph_ids[1], &task)
        .await?;
    assert_eq!(eid_1, Some(ExternalID("commit_def".to_string())));

    // Batch reverse lookup
    let batch = db
        .external_ids
        .to_external_ids(&namespace, &graph_ids, &task)
        .await?;
    assert_eq!(batch.len(), 2);
    assert_eq!(
        batch[0],
        (graph_ids[0], ExternalID("commit_abc".to_string()))
    );
    assert_eq!(
        batch[1],
        (graph_ids[1], ExternalID("commit_def".to_string()))
    );

    Ok(())
}

#[tokio::test]
async fn add_new_external_ids_with_overlap() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let namespace = ExternalIDNamespace("test/git".to_string());

    // Allocate first two
    let first = db
        .external_ids
        .add_new(
            &namespace,
            &[ExternalID("aaa".to_string()), ExternalID("bbb".to_string())],
            &task,
        )
        .await?;

    // Now add a batch that overlaps: [aaa, bbb] already exist, "ccc" is new
    let second = db
        .external_ids
        .add_new(
            &namespace,
            &[
                ExternalID("aaa".to_string()),
                ExternalID("bbb".to_string()),
                ExternalID("ccc".to_string()),
            ],
            &task,
        )
        .await?;

    // "aaa" and "bbb" should get the same graph_ids as before
    assert_eq!(second[0], first[0]);
    assert_eq!(second[1], first[1]);
    // "ccc" should get the next sequential ID
    assert_eq!(second[2].0, 3);

    Ok(())
}

#[tokio::test]
async fn get_latest_external_id() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");

    let namespace = ExternalIDNamespace("test/git".to_string());

    // No mappings yet
    let latest = db.external_ids.get_latest(&namespace, &task).await?;
    assert!(latest.is_none());

    // Allocate some
    db.external_ids
        .add_new(
            &namespace,
            &[
                ExternalID("first".to_string()),
                ExternalID("second".to_string()),
                ExternalID("third".to_string()),
            ],
            &task,
        )
        .await?;

    // Latest should be the one with highest graph_id
    let latest = db.external_ids.get_latest(&namespace, &task).await?;
    assert_eq!(latest, Some(ExternalID("third".to_string())));

    Ok(())
}

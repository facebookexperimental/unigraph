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

    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };

    db.create_timeline(&TimelineID("my_timeline".to_string()), &config)
        .await?;

    let fetched = db
        .get_timeline_config(&TimelineID("my_timeline".to_string()))
        .await?
        .expect("Timeline should exist");

    // Verify the schema type survived round-trip
    match fetched.schema {
        TimelineSchema::AdjacentDeltas(_) => {} // expected
    }

    Ok(())
}

#[tokio::test]
async fn get_nonexistent_timeline_returns_none() -> Result<()> {
    let db = make_db();

    let result = db
        .get_timeline_config(&TimelineID("nonexistent".to_string()))
        .await?;
    assert!(result.is_none());

    Ok(())
}

#[tokio::test]
async fn list_timelines() -> Result<()> {
    let db = make_db();

    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };

    db.create_timeline(&TimelineID("beta".to_string()), &config)
        .await?;
    db.create_timeline(&TimelineID("alpha".to_string()), &config)
        .await?;
    db.create_timeline(&TimelineID("gamma".to_string()), &config)
        .await?;

    let timelines = db.list_timelines().await?;
    let names: Vec<_> = timelines.iter().map(|t| t.0.as_str()).collect();

    // Should be sorted
    assert_eq!(names, vec!["alpha", "beta", "gamma"]);

    Ok(())
}

#[tokio::test]
async fn frames_ordered_by_timestamp_then_graph_id() -> Result<()> {
    let db = make_db();
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };
    db.create_timeline(&TimelineID("test".to_string()), &config)
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
        db.store_graph_full(&key, &graph).await?;
    }

    let frames = db.list_frames(&TimelineID("test".to_string())).await?;
    let ids: Vec<_> = frames.iter().map(|f| f.frame.graph_id.0).collect();

    // Same timestamp → ordered by graph_id
    assert_eq!(ids, vec![1, 2, 3]);

    Ok(())
}

#[tokio::test]
async fn list_frames_range() -> Result<()> {
    let db = make_db();
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };
    db.create_timeline(&TimelineID("test".to_string()), &config)
        .await?;

    for i in 0..10 {
        let graph = TestGraphTimeline::get_nth(i);
        let key = make_graph_time_key("test", i as i64, 1000 + i as i64);
        db.store_graph_full(&key, &graph).await?;
    }

    // Query a range that should include frames 3-7
    let start = unigraph_timestamp::Timestamp::from_unix_timestamp(1003);
    let end = unigraph_timestamp::Timestamp::from_unix_timestamp(1007);

    let frames = db
        .list_frames_range(&TimelineID("test".to_string()), start, end)
        .await?;

    let ids: Vec<_> = frames.iter().map(|f| f.frame.graph_id.0).collect();
    assert_eq!(ids, vec![3, 4, 5, 6, 7]);

    Ok(())
}

#[tokio::test]
async fn add_new_external_ids_allocates_sequentially() -> Result<()> {
    let db = make_db();

    let namespace = ExternalIDNamespace("test/git".to_string());
    let external_ids = vec![
        ExternalID("aaa".to_string()),
        ExternalID("bbb".to_string()),
        ExternalID("ccc".to_string()),
    ];

    let graph_ids = db.add_new_external_ids(&namespace, &external_ids).await?;

    assert_eq!(graph_ids.len(), 3);
    assert_eq!(graph_ids[0].0, 1);
    assert_eq!(graph_ids[1].0, 2);
    assert_eq!(graph_ids[2].0, 3);

    Ok(())
}

#[tokio::test]
async fn add_new_external_ids_is_idempotent() -> Result<()> {
    let db = make_db();

    let namespace = ExternalIDNamespace("test/git".to_string());
    let external_ids = vec![ExternalID("aaa".to_string()), ExternalID("bbb".to_string())];

    // First call: allocate
    let first = db.add_new_external_ids(&namespace, &external_ids).await?;

    // Second call: same inputs, same outputs
    let second = db.add_new_external_ids(&namespace, &external_ids).await?;

    assert_eq!(first, second);

    Ok(())
}

#[tokio::test]
async fn external_id_mapping_roundtrip() -> Result<()> {
    let db = make_db();

    let namespace = ExternalIDNamespace("test/git".to_string());
    let external_ids = vec![
        ExternalID("commit_abc".to_string()),
        ExternalID("commit_def".to_string()),
    ];

    // Allocate
    let graph_ids = db.add_new_external_ids(&namespace, &external_ids).await?;

    // Reverse lookup: graph_id → external_id
    let eid_0 = db
        .graph_id_to_external_id(&namespace, &graph_ids[0])
        .await?;
    assert_eq!(eid_0, Some(ExternalID("commit_abc".to_string())));

    let eid_1 = db
        .graph_id_to_external_id(&namespace, &graph_ids[1])
        .await?;
    assert_eq!(eid_1, Some(ExternalID("commit_def".to_string())));

    // Batch reverse lookup
    let batch = db.graph_ids_to_external_ids(&namespace, &graph_ids).await?;
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

    let namespace = ExternalIDNamespace("test/git".to_string());

    // Allocate first two
    let first = db
        .add_new_external_ids(
            &namespace,
            &[ExternalID("aaa".to_string()), ExternalID("bbb".to_string())],
        )
        .await?;

    // Now add a batch that overlaps: [aaa, bbb] already exist, "ccc" is new
    let second = db
        .add_new_external_ids(
            &namespace,
            &[
                ExternalID("aaa".to_string()),
                ExternalID("bbb".to_string()),
                ExternalID("ccc".to_string()),
            ],
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

    let namespace = ExternalIDNamespace("test/git".to_string());

    // No mappings yet
    let latest = db.get_latest_external_id(&namespace).await?;
    assert!(latest.is_none());

    // Allocate some
    db.add_new_external_ids(
        &namespace,
        &[
            ExternalID("first".to_string()),
            ExternalID("second".to_string()),
            ExternalID("third".to_string()),
        ],
    )
    .await?;

    // Latest should be the one with highest graph_id
    let latest = db.get_latest_external_id(&namespace).await?;
    assert_eq!(latest, Some(ExternalID("third".to_string())));

    Ok(())
}

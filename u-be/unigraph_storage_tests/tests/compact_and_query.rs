// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Comprehensive test exercising compaction and FrameQuery together.
//!
//! 1. Store 12 Full frames with distinct timestamps
//! 2. Snapshot the initial table
//! 3. Compact only the middle range (frames 3–8)
//! 4. Snapshot the table after partial compaction
//! 5. Run various FrameQuery selections and snapshot results

use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;
use unigraph_timestamp::Timestamp;

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

fn ts(seconds: i64) -> Timestamp {
    Timestamp::from_unix_timestamp(seconds)
}

fn tid() -> TimelineID {
    TimelineID("tl".to_string())
}

#[tokio::test]
async fn compact_and_select_frames() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    db.timelines
        .create(
            &tid(),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: None,
            },
            &task,
        )
        .await?;

    // ---------------------------------------------------------------
    // 1. Store 12 Full frames, graph_ids 0..12, timestamps 1000..1011
    // ---------------------------------------------------------------
    let graphs: Vec<_> = (0..12).map(TestGraphTimeline::get_nth).collect();
    let keys: Vec<_> = (0..12)
        .map(|i| make_graph_time_key("tl", i as i64, 1000 + i as i64))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        db.graph.store(key, &graphs[i], &task).await?;
    }

    // ---------------------------------------------------------------
    // 2. Snapshot: all 12 frames are Full
    // ---------------------------------------------------------------
    let all = db.frames.list(&tid(), &task).await?;
    snapshot!(
        format_frames_table(&all),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Full -          -
2                    1970-01-01T00:16:42.000Z Full -          -
3                    1970-01-01T00:16:43.000Z Full -          -
4                    1970-01-01T00:16:44.000Z Full -          -
5                    1970-01-01T00:16:45.000Z Full -          -
6                    1970-01-01T00:16:46.000Z Full -          -
7                    1970-01-01T00:16:47.000Z Full -          -
8                    1970-01-01T00:16:48.000Z Full -          -
9                    1970-01-01T00:16:49.000Z Full -          -
10                   1970-01-01T00:16:50.000Z Full -          -
11                   1970-01-01T00:16:51.000Z Full -          -
"
    );

    // ---------------------------------------------------------------
    // 3. Compact only the middle range: timestamps 1003..=1008
    //    (frames with graph_ids 3, 4, 5, 6, 7, 8)
    //    Frame 3 stays Full (first in range), 4–8 become Deltas.
    // ---------------------------------------------------------------
    let converted = db
        .graph
        .compact(&tid(), Some(ts(1003)), Some(ts(1008)), &task)
        .await?;
    assert_eq!(converted, 5);

    // ---------------------------------------------------------------
    // 4. Snapshot after partial compaction
    // ---------------------------------------------------------------
    let all = db.frames.list(&tid(), &task).await?;
    snapshot!(
        format_frames_table(&all),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Full -          -
2                    1970-01-01T00:16:42.000Z Full -          -
3                    1970-01-01T00:16:43.000Z Full -          -
4                    1970-01-01T00:16:44.000Z Delta tl:3       -
5                    1970-01-01T00:16:45.000Z Delta tl:4       -
6                    1970-01-01T00:16:46.000Z Delta tl:5       -
7                    1970-01-01T00:16:47.000Z Delta tl:6       -
8                    1970-01-01T00:16:48.000Z Delta tl:7       -
9                    1970-01-01T00:16:49.000Z Full -          -
10                   1970-01-01T00:16:50.000Z Full -          -
11                   1970-01-01T00:16:51.000Z Full -          -
"
    );

    // Verify all 12 graphs are still fetchable after partial compaction
    for (i, key) in keys.iter().enumerate() {
        let fetched = db.graph.fetch(&key.graph_key(), &task).await?;
        assert_graphs_equal(&graphs[i], &fetched);
    }

    // ---------------------------------------------------------------
    // 5. FrameQuery selections
    // ---------------------------------------------------------------

    let mut conn = db.graph_conn().await?;

    // 5a. Select only Full frames
    let full_only = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                frame_types: Some(vec![FrameType::Full]),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&full_only),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Full -          -
2                    1970-01-01T00:16:42.000Z Full -          -
3                    1970-01-01T00:16:43.000Z Full -          -
9                    1970-01-01T00:16:49.000Z Full -          -
10                   1970-01-01T00:16:50.000Z Full -          -
11                   1970-01-01T00:16:51.000Z Full -          -
"
    );

    // 5b. Select only Delta frames
    let deltas = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                frame_types: Some(vec![FrameType::Delta]),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&deltas),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
4                    1970-01-01T00:16:44.000Z Delta tl:3       -
5                    1970-01-01T00:16:45.000Z Delta tl:4       -
6                    1970-01-01T00:16:46.000Z Delta tl:5       -
7                    1970-01-01T00:16:47.000Z Delta tl:6       -
8                    1970-01-01T00:16:48.000Z Delta tl:7       -
"
    );

    // 5c. Time range: only frames with timestamp 1005..=1009
    let time_range = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                timestamp_bounds: Some(TimestampBounds {
                    start: Some(ts(1005)),
                    end: Some(ts(1009)),
                }),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&time_range),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
5                    1970-01-01T00:16:45.000Z Delta tl:4       -
6                    1970-01-01T00:16:46.000Z Delta tl:5       -
7                    1970-01-01T00:16:47.000Z Delta tl:6       -
8                    1970-01-01T00:16:48.000Z Delta tl:7       -
9                    1970-01-01T00:16:49.000Z Full -          -
"
    );

    // 5d. Last 3 frames (desc order, limit 3)
    let last_3 = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                order: Some(Order::Desc),
                limit: Some(3),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&last_3),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
11                   1970-01-01T00:16:51.000Z Full -          -
10                   1970-01-01T00:16:50.000Z Full -          -
9                    1970-01-01T00:16:49.000Z Full -          -
"
    );

    // 5e. graph_id bounds: 2..=6
    let id_range = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                graph_id_bounds: Some((Some(GraphID(2)), Some(GraphID(6)))),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&id_range),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
2                    1970-01-01T00:16:42.000Z Full -          -
3                    1970-01-01T00:16:43.000Z Full -          -
4                    1970-01-01T00:16:44.000Z Delta tl:3       -
5                    1970-01-01T00:16:45.000Z Delta tl:4       -
6                    1970-01-01T00:16:46.000Z Delta tl:5       -
"
    );

    // 5f. Specific graph_ids (cherry-pick)
    let cherry = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                graph_ids: Some(vec![GraphID(0), GraphID(5), GraphID(11)]),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&cherry),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
5                    1970-01-01T00:16:45.000Z Delta tl:4       -
11                   1970-01-01T00:16:51.000Z Full -          -
"
    );

    // 5g. "before" — frame immediately preceding graph_id=6 at ts=1006
    let preceding = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                before: Some((ts(1006), GraphID(6))),
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&preceding),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
5                    1970-01-01T00:16:45.000Z Delta tl:4       -
"
    );

    // 5h. Combined: Full frames in time range 1000..=1005, limit 2
    let combined = conn
        .select_frames(
            &FrameQuery {
                timeline_id: tid(),
                frame_types: Some(vec![FrameType::Full]),
                timestamp_bounds: Some(TimestampBounds {
                    start: Some(ts(1000)),
                    end: Some(ts(1005)),
                }),
                limit: Some(2),
                before: None,
                expires_before: None,
                ..Default::default()
            },
            &task,
        )
        .await?;
    snapshot!(
        format_frames_table(&combined),
        "
graph_id             timestamp                type       base       expires_at
----------------------------------------------------------------------------------------------
0                    1970-01-01T00:16:40.000Z Full -          -
1                    1970-01-01T00:16:41.000Z Full -          -
"
    );

    Ok(())
}

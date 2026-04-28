// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Timeline frame statistics — shareable execution logic.
//!
//! The [`run_timeline_stats`] function can be called from any CLI that has
//! access to a [`UnigraphDb`](unigraph_db::UnigraphDb). It queries frame
//! counts by type across several time windows and returns a JSON value
//! suitable for CI recording.

use std::collections::BTreeMap;

use anyhow::Result;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimestampBounds;
use unigraph_timestamp::Timestamp;

/// Named time windows to report on.
const WINDOWS: &[(&str, Option<usize>)] = &[
    ("24h", Some(1)),
    ("7d", Some(7)),
    ("30d", Some(30)),
    ("all", None),
];

/// Query frame-type statistics for a timeline across time windows.
///
/// Uses `conn_analytics` (via `select_analytics`) for all frame queries so
/// the connection gets longer timeouts / a dedicated pool when available.
pub async fn run_timeline_stats(
    timeline_id: &str,
    db: &unigraph_db::UnigraphDb,
    task: &ll::Task,
) -> Result<serde_json::Value> {
    let timeline_id_typed = TimelineID(timeline_id.to_string());
    let now = Timestamp::now();

    let mut windows = serde_json::Map::new();

    for &(label, days) in WINDOWS {
        let query = FrameQuery {
            timeline_id: timeline_id_typed.clone(),
            timestamp_bounds: days.map(|d| {
                let start = now.subtract_days(d).expect("timestamp subtract failed");
                TimestampBounds {
                    start: Some(start),
                    end: Some(now),
                }
            }),
            ..Default::default()
        };

        let frames = db.frames.select_analytics(&query, task).await?;

        let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
        for frame in &frames {
            *by_type.entry(frame.frame_type.to_string()).or_default() += 1;
        }

        windows.insert(
            label.to_string(),
            serde_json::json!({
                "total": frames.len(),
                "by_type": by_type,
            }),
        );
    }

    Ok(serde_json::json!({
        "timeline_id": timeline_id,
        "generated_at": now.to_rfc3339(),
        "windows": windows,
    }))
}

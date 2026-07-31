// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Timeline frame statistics — shareable execution logic.
//!
//! The [`run_timeline_stats`] function can be called from any CLI that has
//! access to a [`UnigraphDb`](unigraph_db::UnigraphDb). It queries frame
//! counts by type across several time windows and returns a [`TimelineStats`]
//! suitable for CI recording or ODS logging.

use std::collections::BTreeMap;

use anyhow::Result;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameType;
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

/// Every frame type, so that a window containing none of a given type reports
/// an explicit zero rather than omitting the entry.
const FRAME_TYPES: &[FrameType] = &[
    FrameType::Empty,
    FrameType::Full,
    FrameType::Delta,
    FrameType::Error,
];

/// Frame counts for a single time window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowStats {
    /// Frame count per [`FrameType`], keyed by its string form.
    pub by_type: BTreeMap<String, usize>,
    /// Total number of frames in the window.
    pub total: usize,
}

/// Frame statistics for one timeline across every reported time window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineStats {
    pub generated_at: String,
    pub timeline_id: String,
    /// Keyed by window label — one of [`WINDOWS`].
    pub windows: BTreeMap<String, WindowStats>,
}

/// Query frame-type statistics for a timeline across time windows.
///
/// Uses `conn_analytics` (via `select_analytics`) for all frame queries so
/// the connection gets longer timeouts / a dedicated pool when available.
pub async fn run_timeline_stats(
    timeline_id: &str,
    db: &unigraph_db::UnigraphDb,
    task: &ll::Task,
) -> Result<TimelineStats> {
    let timeline_id_typed = TimelineID(timeline_id.to_string());
    let now = Timestamp::now();

    let mut windows = BTreeMap::new();

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

        let mut by_type: BTreeMap<String, usize> =
            FRAME_TYPES.iter().map(|ft| (ft.to_string(), 0)).collect();
        for frame in &frames {
            *by_type.entry(frame.frame_type.to_string()).or_default() += 1;
        }

        windows.insert(
            label.to_string(),
            WindowStats {
                by_type,
                total: frames.len(),
            },
        );
    }

    Ok(TimelineStats {
        generated_at: now.to_rfc3339(),
        timeline_id: timeline_id.to_string(),
        windows,
    })
}

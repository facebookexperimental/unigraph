// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use clap::Parser;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;
use unigraph_storage_core::format_frames_table;

use crate::UnigraphCLIContext;

/// List frames in a timeline with optional filters.
///
/// Displays an ASCII table of frames with graph ID, timestamp, frame type,
/// and delta base. Use `--json` for machine-readable output.
///
/// Examples:
///
/// ```sh
/// # List all frames
/// unigraph timelines frames my_timeline
///
/// # List as JSON
/// unigraph timelines frames my_timeline --json
///
/// # Only Full frames from the last 7 days
/// unigraph timelines frames my_timeline --max-days-old 7 --frame-type Full
///
/// # Time range, newest first
/// unigraph timelines frames my_timeline --min-timestamp 2025-01-01T00:00:00Z --desc
///
/// # 10 most recent frames
/// unigraph timelines frames my_timeline --desc --limit 10
/// ```
#[derive(Parser, Debug)]
pub struct TimelinesFrames {
    /// Timeline ID to list frames for
    timeline_id: String,

    /// Print JSON instead of an ASCII table
    #[arg(long)]
    json: bool,

    /// Minimum timestamp (inclusive, RFC 3339). Cannot be combined with --max-days-old
    #[arg(long)]
    min_timestamp: Option<String>,

    /// Maximum timestamp (inclusive, RFC 3339). Cannot be combined with --min-days-old
    #[arg(long)]
    max_timestamp: Option<String>,

    /// Only include frames at most this many days old. Cannot be combined with --min-timestamp
    #[arg(long)]
    max_days_old: Option<u64>,

    /// Only include frames at least this many days old. Cannot be combined with --max-timestamp
    #[arg(long)]
    min_days_old: Option<u64>,

    /// Minimum graph ID (inclusive)
    #[arg(long)]
    min_id: Option<i64>,

    /// Maximum graph ID (inclusive)
    #[arg(long)]
    max_id: Option<i64>,

    /// Filter by frame type (Full, Delta, Error, Empty). Repeatable
    #[arg(long)]
    frame_type: Vec<FrameType>,

    /// Maximum number of frames to return
    #[arg(long)]
    limit: Option<i64>,

    /// Sort newest first (default is oldest first)
    #[arg(long)]
    desc: bool,
}

impl TimelinesFrames {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let query = build_query(self)?;
        let frames = ctx.db.frames.select(&query, task).await?;

        if self.json {
            print_json(ctx, &frames)?;
        } else {
            print_table(ctx, &frames)?;
        }

        task.data("count", frames.len());
        Ok(())
    }
}

fn build_query(args: &TimelinesFrames) -> anyhow::Result<FrameQuery> {
    let timestamp_bounds = resolve_timestamp_bounds(
        args.min_timestamp.as_deref(),
        args.max_timestamp.as_deref(),
        args.max_days_old,
        args.min_days_old,
    )?;

    let graph_id_bounds = match (args.min_id, args.max_id) {
        (None, None) => None,
        (lo, hi) => Some((lo.map(GraphID), hi.map(GraphID))),
    };

    let frame_types = if args.frame_type.is_empty() {
        None
    } else {
        Some(args.frame_type.clone())
    };

    let order = if args.desc { Some(Order::Desc) } else { None };

    Ok(FrameQuery {
        timeline_id: TimelineID(args.timeline_id.clone()),
        limit: args.limit,
        frame_types,
        order,
        timestamp_bounds,
        graph_id_bounds,
        graph_ids: None,
        with_manifest: None,
        with_data: None,
        before: None,
        expires_before: None,
    })
}

fn resolve_timestamp_bounds(
    min_ts: Option<&str>,
    max_ts: Option<&str>,
    max_days_old: Option<u64>,
    min_days_old: Option<u64>,
) -> anyhow::Result<Option<TimestampBounds>> {
    anyhow::ensure!(
        !(min_ts.is_some() && max_days_old.is_some()),
        "--min-timestamp and --max-days-old are mutually exclusive"
    );
    anyhow::ensure!(
        !(max_ts.is_some() && min_days_old.is_some()),
        "--max-timestamp and --min-days-old are mutually exclusive"
    );

    let now = Timestamp::now();

    let start: Option<Timestamp> = match (min_ts, max_days_old) {
        (Some(s), _) => Some(Timestamp::from_rfc3339(s)?),
        (_, Some(days)) => Some(now.subtract_days(days as usize)?),
        _ => None,
    };

    let end: Option<Timestamp> = match (max_ts, min_days_old) {
        (Some(s), _) => Some(Timestamp::from_rfc3339(s)?),
        (_, Some(days)) => Some(now.subtract_days(days as usize)?),
        _ => None,
    };

    match (&start, &end) {
        (None, None) => Ok(None),
        _ => Ok(Some(TimestampBounds { start, end })),
    }
}

fn print_table(ctx: &UnigraphCLIContext, frames: &[FrameRow]) -> anyhow::Result<()> {
    let table = format_frames_table(frames);
    ctx.println_after_done(&table)?;
    ctx.eprintln_after_done(&format!("{} frame(s)", frames.len()))?;
    Ok(())
}

fn print_json(ctx: &UnigraphCLIContext, frames: &[FrameRow]) -> anyhow::Result<()> {
    let entries: Vec<_> = frames.iter().map(frame_to_json).collect();
    let json = serde_json::to_string_pretty(&entries).context("failed to serialize frames")?;
    ctx.println_after_done(&json)?;
    Ok(())
}

fn frame_to_json(frame: &FrameRow) -> serde_json::Value {
    serde_json::json!({
        "graph_id": frame.frame.graph_id.0,
        "timestamp": frame.frame.timestamp.to_rfc3339(),
        "frame_type": frame.frame_type.to_string(),
        "base": frame.base.as_ref().map(|k| format!("{}~{}", k.timeline_id.0, k.graph_id.0)),
    })
}

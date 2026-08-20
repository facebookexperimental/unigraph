// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;
use unigraph_storage_core::TimestampedError;

use crate::Unigraph;

/// Upper bound on how many error frames are resolved at once. Each resolution
/// is a full-data row read plus blob resolution, so the fan-out is capped
/// rather than spanning the whole result page.
const ERROR_FETCH_CONCURRENCY: usize = 8;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SelectFramesInput {
    pub timeline_id: TimelineID,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    /// Inclusive lower bound on frame timestamp, RFC3339 (e.g. `2026-08-05T16:00:00Z`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_start: Option<String>,
    /// Inclusive upper bound on frame timestamp, RFC3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_end: Option<String>,
    /// Only return frames with these graph_ids. Compiles to a SQL `IN`, so it
    /// answers "which of these graphs exist" in one round trip rather than
    /// paging a busy timeline looking for them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_ids: Option<Vec<i64>>,
    /// Populate [`FrameInfo::error`] for `Error` frames. Off by default — each
    /// error frame costs a full-data row read plus blob resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_error_info: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SelectFramesOutput {
    pub frames: Vec<FrameInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FrameInfo {
    pub graph_id: i64,
    pub timestamp: String,
    pub frame_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Resolved error content, only for `Error` frames and only when the
    /// request set `include_error_info`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FrameErrorInfo>,
}

/// The error payload stored on an `Error` frame.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FrameErrorInfo {
    pub error_count: i64,
    pub errors: Vec<FrameError>,
}

/// A single timestamped error message from a failed graph computation.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FrameError {
    pub timestamp: String,
    pub message: String,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for SelectFramesInput {
    type Output = SelectFramesOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<SelectFramesOutput> {
        let query = to_frame_query(&self)?;
        let rows = ctx.db.frames.select(&query, task).await?;
        let mut frames: Vec<FrameInfo> = rows.iter().map(to_frame_info).collect();

        if self.include_error_info == Some(true) {
            attach_error_info(ctx, &rows, &mut frames, task).await?;
        }

        Ok(SelectFramesOutput { frames })
    }
}

fn to_frame_query(input: &SelectFramesInput) -> Result<FrameQuery> {
    let frame_types = input
        .frame_types
        .as_ref()
        .map(|types| types.iter().map(|s| s.parse()).collect::<Result<Vec<_>>>())
        .transpose()?;

    let order = input
        .order
        .as_deref()
        .map(|s| match s {
            "Asc" | "asc" => Ok(Order::Asc),
            "Desc" | "desc" => Ok(Order::Desc),
            other => anyhow::bail!("unknown order: {other}"),
        })
        .transpose()?;

    let timestamp_bounds = to_timestamp_bounds(
        input.timestamp_start.as_deref(),
        input.timestamp_end.as_deref(),
    )?;

    Ok(FrameQuery {
        timeline_id: input.timeline_id.clone(),
        limit: input.limit,
        frame_types,
        order,
        timestamp_bounds,
        graph_id_bounds: None,
        graph_ids: input
            .graph_ids
            .as_ref()
            .map(|ids| ids.iter().copied().map(GraphID).collect()),
        with_data: None,
        before: None,
        expires_before: None,
    })
}

/// Parse the RFC3339 bounds into [`TimestampBounds`].
///
/// Returns `None` when neither bound is set so the query imposes no timestamp
/// constraint at all, rather than an all-`None` bounds struct.
fn to_timestamp_bounds(start: Option<&str>, end: Option<&str>) -> Result<Option<TimestampBounds>> {
    if start.is_none() && end.is_none() {
        return Ok(None);
    }

    let start = start.map(Timestamp::from_rfc3339).transpose()?;
    let end = end.map(Timestamp::from_rfc3339).transpose()?;

    Ok(Some(TimestampBounds { start, end }))
}

fn to_frame_info(row: &FrameRow) -> FrameInfo {
    FrameInfo {
        graph_id: row.frame.graph_id.0,
        timestamp: row.frame.timestamp.to_rfc3339(),
        frame_type: row.frame_type.to_string(),
        base: row
            .base
            .as_ref()
            .map(|k| format!("{}~{}", k.timeline_id.0, k.graph_id.0)),
        error: None,
    }
}

/// Resolve and attach the error payload of every `Error` frame in `rows`.
///
/// `rows` and `frames` are positionally aligned — `frames` is built by mapping
/// over `rows`, so index `i` refers to the same frame in both.
async fn attach_error_info(
    ctx: &Unigraph,
    rows: &[FrameRow],
    frames: &mut [FrameInfo],
    task: &ll::Task,
) -> Result<()> {
    let error_frames: Vec<(usize, GraphKey)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.frame_type == FrameType::Error)
        .map(|(idx, row)| (idx, to_graph_key(row)))
        .collect();

    let resolved: Vec<(usize, FrameErrorInfo)> = stream::iter(error_frames)
        .map(|(idx, key)| async move {
            let errors = ctx.db.graph.fetch_errors(&key, task).await?;
            anyhow::Ok((idx, to_frame_error_info(&errors)))
        })
        .buffer_unordered(ERROR_FETCH_CONCURRENCY)
        .try_collect()
        .await?;

    for (idx, info) in resolved {
        frames[idx].error = Some(info);
    }

    Ok(())
}

fn to_graph_key(row: &FrameRow) -> GraphKey {
    GraphKey {
        timeline_id: row.timeline_id.clone(),
        graph_id: row.frame.graph_id,
    }
}

fn to_frame_error_info(errors: &[TimestampedError]) -> FrameErrorInfo {
    FrameErrorInfo {
        error_count: errors.len() as i64,
        errors: errors
            .iter()
            .map(|e| FrameError {
                timestamp: e.timestamp.to_rfc3339(),
                message: e.message.clone(),
            })
            .collect(),
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use unigraph_storage_core::FrameType;

    use super::*;

    fn input() -> SelectFramesInput {
        SelectFramesInput {
            timeline_id: TimelineID("www".to_owned()),
            limit: None,
            frame_types: None,
            order: None,
            timestamp_start: None,
            timestamp_end: None,
            graph_ids: None,
            include_error_info: None,
        }
    }

    #[test]
    fn no_bounds_yields_no_constraint() {
        let query = to_frame_query(&input()).expect("query with no bounds should build");
        assert!(
            query.timestamp_bounds.is_none(),
            "Omitting both bounds should leave timestamp_bounds unset, not an all-None struct"
        );
    }

    #[test]
    fn both_bounds_are_parsed() {
        let query = to_frame_query(&SelectFramesInput {
            timestamp_start: Some("2026-08-01T00:00:00Z".to_owned()),
            timestamp_end: Some("2026-08-05T00:00:00Z".to_owned()),
            ..input()
        })
        .expect("valid RFC3339 bounds should parse");

        let bounds = query.timestamp_bounds.expect("bounds should be set");
        assert_eq!(
            bounds.start.expect("start should be set").to_rfc3339(),
            Timestamp::from_rfc3339("2026-08-01T00:00:00Z")
                .expect("test literal is valid RFC3339")
                .to_rfc3339(),
        );
        assert_eq!(
            bounds.end.expect("end should be set").to_rfc3339(),
            Timestamp::from_rfc3339("2026-08-05T00:00:00Z")
                .expect("test literal is valid RFC3339")
                .to_rfc3339(),
        );
    }

    #[test]
    fn a_single_bound_leaves_the_other_open() {
        let query = to_frame_query(&SelectFramesInput {
            timestamp_start: Some("2026-08-01T00:00:00Z".to_owned()),
            ..input()
        })
        .expect("a lone start bound should parse");

        let bounds = query.timestamp_bounds.expect("bounds should be set");
        assert!(bounds.start.is_some(), "start should be set");
        assert!(
            bounds.end.is_none(),
            "An unset end bound must stay open-ended"
        );
    }

    #[test]
    fn malformed_timestamp_is_rejected() {
        let err = to_frame_query(&SelectFramesInput {
            timestamp_start: Some("last tuesday".to_owned()),
            ..input()
        })
        .expect_err("a non-RFC3339 bound should be rejected rather than silently ignored");
        assert!(
            format!("{err:#}").contains("rfc3339"),
            "Error should mention the expected format, got: {err:#}"
        );
    }

    #[test]
    fn graph_ids_become_an_in_filter() {
        let query = to_frame_query(&SelectFramesInput {
            graph_ids: Some(vec![416802146, 416804233]),
            ..input()
        })
        .expect("an explicit graph_id list should build");

        assert_eq!(
            query.graph_ids.as_deref(),
            Some(&[GraphID(416802146), GraphID(416804233)][..]),
            "Requested graph_ids should reach the query so the DB does the IN, not the caller"
        );
    }

    #[test]
    fn omitted_graph_ids_impose_no_constraint() {
        let query = to_frame_query(&input()).expect("query with no graph_ids should build");
        assert!(
            query.graph_ids.is_none(),
            "Omitting graph_ids must not narrow the query to the empty set"
        );
    }

    #[test]
    fn timestamp_bounds_compose_with_other_filters() {
        let query = to_frame_query(&SelectFramesInput {
            limit: Some(10),
            frame_types: Some(vec!["Error".to_owned()]),
            order: Some("Desc".to_owned()),
            timestamp_end: Some("2026-08-05T00:00:00Z".to_owned()),
            ..input()
        })
        .expect("bounds should compose with the pre-existing filters");

        assert_eq!(query.limit, Some(10));
        assert_eq!(query.frame_types.as_deref(), Some(&[FrameType::Error][..]));
        assert!(matches!(query.order, Some(Order::Desc)));
        assert!(query.timestamp_bounds.is_some_and(|b| b.end.is_some()));
    }
}

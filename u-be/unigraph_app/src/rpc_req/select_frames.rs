// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::Unigraph;

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
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for SelectFramesInput {
    type Output = SelectFramesOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<SelectFramesOutput> {
        let query = to_frame_query(&self)?;
        let rows = ctx.db.frames.select(&query, task).await?;
        let frames = rows.iter().map(to_frame_info).collect();
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
        graph_ids: None,
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

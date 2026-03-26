// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameRow;
use unigraph_storage_core::Order;

use crate::FrameInfo;
use crate::SelectFramesInput;
use crate::SelectFramesOutput;
use crate::Unigraph;

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

    Ok(FrameQuery {
        timeline_id: input.timeline_id.clone(),
        limit: input.limit,
        frame_types,
        order,
        timestamp_bounds: None,
        graph_id_bounds: None,
        graph_ids: None,
        with_data: None,
        before: None,
        expires_before: None,
    })
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::TimelineID;

use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ListTimelinesInput {}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ListTimelinesOutput {
    pub timeline_ids: Vec<TimelineID>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for ListTimelinesInput {
    type Output = ListTimelinesOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<ListTimelinesOutput> {
        let timeline_ids = ctx.db.timelines.list(task).await?;
        Ok(ListTimelinesOutput { timeline_ids })
    }
}

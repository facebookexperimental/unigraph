// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use unigraph_rpc::RpcExec;

use crate::ListTimelinesInput;
use crate::ListTimelinesOutput;
use crate::Unigraph;

impl RpcExec<Unigraph> for ListTimelinesInput {
    type Output = ListTimelinesOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<ListTimelinesOutput> {
        let timeline_ids = ctx.db.timelines.list(task).await?;
        Ok(ListTimelinesOutput { timeline_ids })
    }
}

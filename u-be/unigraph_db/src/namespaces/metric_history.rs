// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Metric history queries — fetch per-node metric time series.

use std::collections::BTreeMap;

use anyhow::Result;
use ll::task;
use unigraph_metric_history::NodeMetricSnapshot;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;

use crate::context::UnigraphDbContext;

/// Handle for metric history operations.
///
/// Obtained via [`UnigraphDb::metric_history`](crate::UnigraphDb).
#[derive(Clone)]
pub struct MetricHistory {
    pub(crate) ctx: UnigraphDbContext,
}

impl MetricHistory {
    /// Fetch metric history for specific nodes within a time range.
    ///
    /// Returns a map of `node_name → Vec<(Timestamp, GraphID, NodeMetricSnapshot)>`,
    /// sorted by `(Timestamp, GraphID)`, deduplicated across week boundaries.
    #[task(tags(l3))]
    pub async fn fetch(
        &self,
        timeline_id: &TimelineID,
        node_names: &[String],
        start: Timestamp,
        end: Timestamp,
        task: &ll::Task,
    ) -> Result<BTreeMap<String, Vec<(Timestamp, GraphID, NodeMetricSnapshot)>>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        crate::metric_history::fetch_metric_history(
            &mut *conn,
            timeline_id,
            node_names,
            start,
            end,
            &task,
        )
        .await
    }
}

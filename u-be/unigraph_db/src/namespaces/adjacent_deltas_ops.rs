// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Adjacent deltas batch operations — store and load graph ranges.

use anyhow::Result;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::TimelineID;

use crate::context::UnigraphDbContext;
use crate::graph_range::GraphRange;

/// Handle for adjacent deltas batch operations (store/load graph ranges).
///
/// Obtained via [`Graph::adjacent_deltas`](crate::Graph).
#[derive(Clone)]
pub struct AdjacentDeltasOps {
    pub(crate) ctx: UnigraphDbContext,
}

impl AdjacentDeltasOps {
    /// Store a [`GraphRange`] atomically with CAS semantics.
    ///
    /// Consumes the range. Packs each entry, verifies all target frames are
    /// Empty, then stores the entire range in a single transaction.
    #[ll::task(tags(l3))]
    pub async fn store_range(&self, range: GraphRange, task: &ll::Task) -> Result<()> {
        crate::schemas::adjacent_deltas::store_range(&self.ctx, range, &task).await
    }

    /// Load a range of frames from storage into a [`GraphRange`].
    ///
    /// Fetches frames, unpacks them into domain data, and returns a
    /// `GraphRange` ready for iteration.
    #[ll::task(tags(l3))]
    pub async fn load_range(
        &self,
        timeline_id: &TimelineID,
        from: Option<GraphID>,
        to: Option<GraphID>,
        task: &ll::Task,
    ) -> Result<GraphRange> {
        crate::schemas::adjacent_deltas::load_range(timeline_id, &self.ctx, from, to, &task).await
    }
}

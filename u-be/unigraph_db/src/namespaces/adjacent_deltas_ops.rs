// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Adjacent deltas batch operations — store and load graph ranges.

use anyhow::Result;
use unigraph_storage_core::Frame;
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
    #[ll::task]
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

    /// Store new empty frames, filtering out frames already present.
    ///
    /// `new_frames` must be sorted by `(timestamp, graph_id)` with strictly
    /// increasing graph_ids and non-decreasing timestamps. The range may
    /// overlap with the tail of already-stored frames — the overlap is
    /// validated for alignment and filtered out.
    ///
    /// When `require_overlap` is `true`, the input MUST overlap with at least
    /// one stored frame (unless the timeline is empty). This prevents silently
    /// appending frames with a gap when the caller expected continuity.
    ///
    /// Runs in a single transaction with an exclusive timeline lock.
    /// Inserts are chunked (10,000 frames per batch) to keep SQL statements
    /// bounded.
    ///
    /// Returns the number of frames actually inserted.
    #[ll::task]
    pub async fn put_new_empty_frames(
        &self,
        timeline_id: &TimelineID,
        new_frames: Vec<Frame>,
        require_overlap: bool,
        task: &ll::Task,
    ) -> Result<usize> {
        crate::schemas::adjacent_deltas::put_new_empty_frames(
            &self.ctx,
            timeline_id,
            new_frames,
            require_overlap,
            &task,
        )
        .await
    }
}

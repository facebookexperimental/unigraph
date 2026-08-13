// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Reclaiming rows, and re-applying a threshold after the fact.
//!
//! Two passes with very different costs, kept apart so the cheap one can run on
//! a schedule and the expensive one only when it is actually needed.
//!
//! ```text
//! sweep_segments      one DELETE per stretch between barriers, all nodes at
//!                     once   -> cost scales with gaps
//! rethreshold_nodes   re-derive each node's crossings and anchors from the
//!                     stored values   -> cost scales with nodes
//! ```
//!
//! The sweep is what reclaims the boundary rows a closing gap released: those
//! rows were written with no reasons at all, held only by their frame's barrier
//! flag, so once the flag goes they match `reasons = 0` and a plain range
//! statement collects them. Nothing has to be read first, and nothing has to be
//! judged.

use std::collections::BTreeSet;

use anyhow::Result;
use ll::task;
use unigraph_storage_core::ExclusiveGraphIDRange;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphIDBounds;
use unigraph_storage_core::TimelineID;

use super::GraphHistory;
use super::HistoryCompactOptions;
use super::HistoryCompactReport;
use super::context::FrameContext;
use super::context::within_bounds;
use super::progress::PROGRESS_WINDOW;
use super::progress::Throughput;
use super::progress::windows;
use crate::graph_history::CompactInput;
use crate::graph_history::CompactRow;
use crate::graph_history::FrameFlags;
use crate::graph_history::Segment;
use crate::graph_history::compact_series;
use crate::graph_history::decode_values;
use crate::graph_history::segments;

impl GraphHistory {
    /// Delete the zero-reason rows in every stretch between barriers.
    #[task]
    pub(super) async fn sweep_segments(
        &self,
        timeline_id: &TimelineID,
        context: &FrameContext,
        options: &HistoryCompactOptions,
        report: &mut HistoryCompactReport,
        task: &ll::Task,
    ) -> Result<()> {
        let gaps = context.gaps();
        let flags = gaps
            .iter()
            .map(|gap| context.flags(gap.graph_id))
            .collect::<Vec<_>>();
        let segments = segments(&gaps, &flags)
            .into_iter()
            .filter_map(|segment| clip(segment, &options.range.graph_ids))
            .collect::<Vec<_>>();

        let total = i64::try_from(segments.len())?;
        task.data("segments", total);
        task.progress(0, total);

        for (index, segment) in segments.iter().enumerate() {
            let mut conn = self.ctx.storage.graph.conn_write().await?;
            conn.start_transaction(&task).await?;
            conn.get_timeline_config_and_lock(timeline_id, &task)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
            let deleted = conn
                .delete_collapsed_history_entries(timeline_id, segment, &task)
                .await?;
            conn.commit_transaction(&task).await?;

            report.collapsed += usize::try_from(deleted)?;
            task.progress(i64::try_from(index)? + 1, total);
        }
        report.segments = segments.len();
        Ok(())
    }

    /// Re-derive every node's crossings and anchors at the requested threshold.
    ///
    /// One series read per node, so bound the range on a wide timeline. Only a
    /// threshold change actually needs this.
    #[task]
    pub(super) async fn rethreshold_nodes(
        &self,
        timeline_id: &TimelineID,
        context: &FrameContext,
        options: &HistoryCompactOptions,
        report: &mut HistoryCompactReport,
        task: &ll::Task,
    ) -> Result<()> {
        let mut conn = self.ctx.storage.graph.conn_analytics().await?;
        let node_names = conn
            .list_history_node_names(timeline_id, &options.range, &task)
            .await?;
        drop(conn);

        let frames = context
            .data_frames
            .iter()
            .copied()
            .filter(|graph_id| within_bounds(*graph_id, &options.range.graph_ids))
            .collect::<Vec<_>>();
        let barriers = context
            .flags
            .iter()
            .filter(|(_, flags)| flags.is_barrier())
            .map(|(graph_id, _)| *graph_id)
            .collect::<BTreeSet<_>>();
        // Two frames can be neighbours in `frames` with a run of unbuilt frames
        // between them, and measuring across that would blame one diff for
        // the whole unknown region.
        let after_gap = context
            .flags
            .iter()
            .filter(|(_, flags)| flags.contains(FrameFlags::AFTER_GAP))
            .map(|(graph_id, _)| *graph_id)
            .collect::<BTreeSet<_>>();

        let total = i64::try_from(node_names.len())?;
        report.nodes = node_names.len();
        task.data("nodes_in_range", total);
        task.progress(0, total);
        let mut rate = Throughput::new();
        let mut done = 0i64;

        // Windowed rather than one child task per node: a timeline can hold
        // hundreds of thousands of nodes, and a task tree event each would cost
        // more than the compaction.
        for (window, nodes) in node_names.chunks(PROGRESS_WINDOW).enumerate() {
            let label = rate.label(i64::try_from(window)? + 1, windows(total), "rows dropped");
            let frames = frames.as_slice();
            let barriers = &barriers;
            let after_gap = &after_gap;
            let counts = task
                .spawn(label, |task| async move {
                    let mut counts = NodeCounts::default();
                    for node_name in nodes {
                        let node = self
                            .rethreshold_node(
                                timeline_id,
                                node_name,
                                options,
                                frames,
                                after_gap,
                                barriers,
                                &task,
                            )
                            .await?;
                        counts.dropped += node.dropped;
                        counts.updated += node.updated;
                    }
                    Ok(counts)
                })
                .await?;
            report.dropped += counts.dropped;
            report.updated += counts.updated;
            rate.add(u64::try_from(counts.dropped)?);
            done += i64::try_from(nodes.len())?;
            task.progress(done, total);
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "a params struct here would just re-list the same borrows with a name"
    )]
    async fn rethreshold_node(
        &self,
        timeline_id: &TimelineID,
        node_name: &str,
        options: &HistoryCompactOptions,
        frames: &[GraphID],
        after_gap: &BTreeSet<GraphID>,
        barriers: &BTreeSet<GraphID>,
        task: &ll::Task,
    ) -> Result<NodeCounts> {
        let mut conn = self.ctx.storage.graph.conn_analytics().await?;
        let rows = conn
            .get_history_series(timeline_id, node_name, &options.range, task)
            .await?;
        drop(conn);

        let series = rows
            .into_iter()
            .map(|row| {
                Ok(CompactRow {
                    graph_id: row.graph_id,
                    values: decode_values(&row.values)?,
                    reasons: row.reasons,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let plan = compact_series(&CompactInput {
            series: &series,
            frames,
            after_gap,
            barriers,
            threshold: options.threshold,
        });
        if plan.is_empty() {
            return Ok(NodeCounts::default());
        }

        let updates = plan.updated.clone();

        let mut conn = self.ctx.storage.graph.conn_write().await?;
        conn.start_transaction(task).await?;
        conn.get_timeline_config_and_lock(timeline_id, task)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Timeline not found: {}", timeline_id))?;
        conn.set_history_entry_reasons(timeline_id, node_name, &updates, task)
            .await?;
        conn.delete_history_entries_for_node(timeline_id, node_name, &plan.dropped, task)
            .await?;
        conn.commit_transaction(task).await?;

        Ok(NodeCounts {
            dropped: plan.dropped.len(),
            updated: plan.updated.len(),
        })
    }
}

#[derive(Default)]
struct NodeCounts {
    dropped: usize,
    updated: usize,
}

/// Narrow a segment to the caller's requested range, or drop it entirely.
///
/// The caller's bounds are inclusive and a segment's are exclusive, so a bound
/// landing inside a segment tightens it by one either way — which is exactly
/// right: a row *at* the bound is in range, and the sweep must not reach past
/// it.
fn clip(segment: Segment, bounds: &GraphIDBounds) -> Option<ExclusiveGraphIDRange> {
    let after = match (segment.after, bounds.0) {
        (Some(after), Some(from)) => Some(after.max(GraphID(from.0 - 1))),
        (after, from) => after.or_else(|| from.map(|from| GraphID(from.0 - 1))),
    };
    let before = match (segment.before, bounds.1) {
        (Some(before), Some(to)) => Some(before.min(GraphID(to.0 + 1))),
        (before, to) => before.or_else(|| to.map(|to| GraphID(to.0 + 1))),
    };
    match (after, before) {
        (Some(after), Some(before)) if after >= before => None,
        _ => Some(ExclusiveGraphIDRange { after, before }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(after: Option<i64>, before: Option<i64>) -> Segment {
        Segment {
            after: after.map(GraphID),
            before: before.map(GraphID),
        }
    }

    fn render(clipped: Option<ExclusiveGraphIDRange>) -> String {
        let Some(range) = clipped else {
            return "-".to_owned();
        };
        let bound = |id: Option<GraphID>| id.map_or_else(|| "-".to_owned(), |id| id.0.to_string());
        format!("({}, {})", bound(range.after), bound(range.before))
    }

    /// A bounded compaction must not reach outside its range, and must not
    /// widen a segment that already stops short of it.
    #[test]
    fn clipping_never_widens_a_segment_or_escapes_the_bounds() {
        let cases = [
            (segment(None, None), (None, None), "(-, -)", "unbounded"),
            (
                segment(Some(10), Some(20)),
                (None, None),
                "(10, 20)",
                "no request bounds leaves the segment alone",
            ),
            (
                segment(None, None),
                (Some(10), Some(20)),
                "(9, 21)",
                "an inclusive request becomes an exclusive sweep one wider",
            ),
            (
                segment(Some(10), Some(40)),
                (Some(20), Some(30)),
                "(19, 31)",
                "the tighter bound wins on each side",
            ),
            (
                segment(Some(20), Some(30)),
                (Some(10), Some(40)),
                "(20, 30)",
                "a request wider than the segment cannot widen it",
            ),
            (
                segment(Some(10), Some(20)),
                (Some(50), Some(60)),
                "-",
                "a segment entirely outside the request is skipped",
            ),
        ];

        let report = cases
            .iter()
            .map(|(segment, bounds, expected, why)| {
                let bounds = (bounds.0.map(GraphID), bounds.1.map(GraphID));
                let got = render(clip(*segment, &bounds));
                assert_eq!(&got, expected, "{why}");
                format!("{got:<10} {why}")
            })
            .collect::<Vec<_>>()
            .join("\n");

        k9::snapshot!(
            report,
            "
(-, -)     unbounded
(10, 20)   no request bounds leaves the segment alone
(9, 21)    an inclusive request becomes an exclusive sweep one wider
(19, 31)   the tighter bound wins on each side
(20, 30)   a request wider than the segment cannot widen it
-          a segment entirely outside the request is skipped
"
        );
    }
}

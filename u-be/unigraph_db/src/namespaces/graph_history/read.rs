// Copyright (c) Meta Platforms, Inc. and affiliates.

//! The read path: stored rows to a chartable series.
//!
//! Two things happen here that storage cannot do on its own — metric ids become
//! names, and each row learns whether the row before it is its immediate frame
//! predecessor. That second one is the bit a chart actually needs, and it is
//! deliberately *not* any single reason: a step is attributable when the two
//! rows it spans are frame-adjacent, which covers an anchor followed by a
//! crossing and, just as importantly, two consecutive crossings — what a
//! landing diff stack looks like, and precisely the case the old `anchor` flag
//! threw away.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use ll::task;
use unigraph_storage_core::FrameQuery;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::HistoryRange;
use unigraph_storage_core::HistorySampleRow;
use unigraph_storage_core::Order;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimestampBounds;

use super::GraphHistory;
use super::HistorySeriesRow;
use crate::graph_history::decode_values;

impl GraphHistory {
    /// Read several nodes' series in one pass, keyed by node name.
    ///
    /// The metric dictionary, the connection and the built-frame sequence are
    /// read once for the whole batch rather than once per node. Duplicate names
    /// collapse to a single entry.
    #[task(tags(l3))]
    pub async fn series_many(
        &self,
        timeline_id: &TimelineID,
        node_names: &[String],
        bounds: &TimestampBounds,
        task: &ll::Task,
    ) -> Result<BTreeMap<String, Vec<HistorySeriesRow>>> {
        let mut conn = self.ctx.storage.graph.conn().await?;
        let metric_names = conn.get_history_metric_names(timeline_id, &task).await?;
        let range = HistoryRange {
            timestamps: bounds.clone(),
            graph_ids: (None, None),
        };

        let mut stored = BTreeMap::new();
        for node_name in node_names {
            let rows = conn
                .get_history_series(timeline_id, node_name, &range, &task)
                .await?;
            stored.insert(node_name.clone(), rows);
        }
        drop(conn);

        let frames = self.frames_spanning(timeline_id, &stored, &task).await?;
        stored
            .into_iter()
            .map(|(node_name, rows)| Ok((node_name, to_series(&metric_names, &frames, rows)?)))
            .collect()
    }

    /// The frame sequence spanning whatever the read actually returned, each
    /// tagged with whether it carries data.
    ///
    /// Every frame type, not just the built ones: attribution is about frames
    /// being *adjacent*, and filtering the unbuilt ones out would close every
    /// hole in the sequence and make a step across an unknown region look like
    /// one diff's work.
    ///
    /// Bounded to the ids in play rather than the whole timeline: this is a
    /// user-facing path, and a chart of a few nodes should not drag the entire
    /// frame list along with it.
    async fn frames_spanning(
        &self,
        timeline_id: &TimelineID,
        stored: &BTreeMap<String, Vec<HistorySampleRow>>,
        task: &ll::Task,
    ) -> Result<Vec<(GraphID, bool)>> {
        let ids = stored
            .values()
            .flatten()
            .map(|row| row.graph_id)
            .collect::<BTreeSet<_>>();
        let (Some(first), Some(last)) = (ids.first(), ids.last()) else {
            return Ok(Vec::new());
        };

        let mut conn = self.ctx.storage.graph.conn().await?;
        let frames = conn
            .select_frames(
                &FrameQuery {
                    timeline_id: timeline_id.clone(),
                    limit: None,
                    frame_types: Some(vec![
                        FrameType::Empty,
                        FrameType::Full,
                        FrameType::Delta,
                        FrameType::Error,
                    ]),
                    order: Some(Order::Asc),
                    timestamp_bounds: None,
                    graph_id_bounds: Some((Some(*first), Some(*last))),
                    graph_ids: None,
                    with_manifest: None,
                    with_data: Some(false),
                    before: None,
                    expires_before: None,
                },
                task,
            )
            .await?;
        Ok(frames
            .into_iter()
            .map(|frame| {
                let has_data = matches!(frame.frame_type, FrameType::Full | FrameType::Delta);
                (frame.frame.graph_id, has_data)
            })
            .collect())
    }
}

/// Decode one node's rows and mark which steps are attributable.
fn to_series(
    metric_names: &BTreeMap<u32, String>,
    frames: &[(GraphID, bool)],
    rows: Vec<HistorySampleRow>,
) -> Result<Vec<HistorySeriesRow>> {
    let mut series: Vec<HistorySeriesRow> = Vec::with_capacity(rows.len());
    for row in rows {
        let attributable = series.last().is_some_and(|previous| {
            preceding_data_frame(row.graph_id, frames) == Some(previous.graph_id)
        });
        series.push(HistorySeriesRow {
            graph_id: row.graph_id,
            timestamp: row.timestamp,
            values: decode_named_values(metric_names, &row.values)?,
            reasons: row.reasons,
            attributable,
        });
    }
    Ok(series)
}

/// The frame immediately before `graph_id`, if it exists and carries data.
///
/// `None` across a gap, which is the whole point: the frame before a gap's far
/// edge is unbuilt, so nothing there is adjacent to anything.
fn preceding_data_frame(graph_id: GraphID, frames: &[(GraphID, bool)]) -> Option<GraphID> {
    let index = frames.partition_point(|(id, _)| *id < graph_id);
    let (previous, has_data) = frames.get(index.checked_sub(1)?)?;
    has_data.then_some(*previous)
}

fn decode_named_values(
    metric_names: &BTreeMap<u32, String>,
    values: &[u8],
) -> Result<BTreeMap<String, f64>> {
    decode_values(values)?
        .into_iter()
        .map(|(metric_id, value)| {
            let name = metric_names
                .get(&metric_id)
                .ok_or_else(|| anyhow::anyhow!("missing metric name for id {metric_id}"))?;
            Ok((name.clone(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use unigraph_storage_core::Timestamp;

    use super::*;
    use crate::graph_history::Reasons;

    fn sample(graph_id: i64, reasons: Reasons) -> HistorySampleRow {
        HistorySampleRow {
            graph_id: GraphID(graph_id),
            timestamp: Timestamp::from_unix_timestamp(graph_id),
            values: Vec::new(),
            reasons,
        }
    }

    /// Attribution is a frame-adjacency question, not a reason question. The
    /// old design read it off the `anchor` flag and so silently discarded every
    /// step between two consecutive crossings — which is exactly what a landing
    /// diff stack produces.
    #[test]
    fn a_step_is_attributable_whenever_the_two_rows_are_frame_adjacent() {
        // Frame 4 is unbuilt, so nothing may be attributed across it.
        let frames = (1..=6)
            .map(|graph_id| (GraphID(graph_id), graph_id != 4))
            .collect::<Vec<_>>();
        let rows = vec![
            sample(1, Reasons::FIRST),
            sample(2, Reasons::OVER_THRESHOLD),
            sample(3, Reasons::OVER_THRESHOLD),
            sample(5, Reasons::empty()),
            sample(6, Reasons::OVER_THRESHOLD),
        ];

        let series = to_series(&BTreeMap::new(), &frames, rows).expect("decodes");
        let report = series
            .iter()
            .map(|row| {
                format!(
                    "{:>2} {:<22} attributable {}",
                    row.graph_id.0, row.reasons, row.attributable
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        k9::snapshot!(
            report,
            "
 1 FIRST                  attributable false
 2 OVER_THRESHOLD         attributable true
 3 OVER_THRESHOLD         attributable true
 5 -                      attributable false
 6 OVER_THRESHOLD         attributable true
"
        );
    }
}

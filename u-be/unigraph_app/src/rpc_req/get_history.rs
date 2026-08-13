// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Read per-node metric history — the RPC equivalent of `unigraph history show`.
//!
//! # Why the wire shape looks like this
//!
//! A history read is the largest response this service produces. Charting a
//! few hundred nodes across a few thousand frames is ~1e6 samples, and each
//! sample carries a timestamp, a frame id and one value per metric. Written
//! the obvious way — one JSON object per sample —
//!
//! ```text
//! {"node":"app","timestamp":"2026-08-05T16:00:00Z","graph_id":41823,
//!  "metrics":{"lines":248284,"size":9932}}
//! ```
//!
//! that is ~110 bytes, of which ~85 are field names and re-typed absolutes. At
//! 1e6 samples it is a ~100 MB string that must exist in the server's heap, in
//! the socket buffer, and in the browser's JSON parser — and we have OOM'd on
//! exactly this before. So the format is built so that the common case costs
//! almost nothing:
//!
//! 1. **Names are sent once.** [`GetHistoryOutput::metrics`] is the only place
//!    a metric name appears; every value is positional.
//! 2. **No per-sample objects.** A node's whole series is one flat numeric
//!    array read in fixed-size chunks — no repeated keys, and no per-sample
//!    allocation on either side of the wire.
//! 3. **Only changes are sent.** Every column is delta-encoded against its own
//!    previous value. Metrics move slowly and frames are evenly spaced, so the
//!    stream is mostly `0`, `1` and `3600`: one to four characters where an
//!    absolute would take ten to twelve, and far more compressible.
//!
//! # Layout
//!
//! [`NodeHistory::deltas`] is a flat array of `stride`-sized chunks, one chunk
//! per sample, in ascending time order. `stride = 4 + metrics.len()`: four
//! header slots, then one slot per metric.
//!
//! ```text
//! metrics: ["lines", "size"]          <- sent once; stride = 4 + 2 = 6
//!
//! series[0].node_name = "app"
//! series[0].deltas    = [1754400000, 0, 1, 0, 10, 100,  3600, 1, 3, 1, 10, 200,  ...]
//!                        └── chunk 0 ─────────────────┘ └── chunk 1 ──────────┘
//!                         ts         gid  ↑  ↑   ↑    ↑
//!                                 reasons  │  lines  size
//!                                   attributable
//! ```
//!
//! Slot `0` is the sample's timestamp in **unix seconds** (not RFC3339 — a
//! quoted timestamp is 25 bytes per sample and needs parsing; a delta from the
//! previous frame is usually a 4-digit integer). Slot `1` is the `graph_id`, so
//! a chart point can link straight back to the frame it came from.
//!
//! Slot `2` is the sample's **reasons** bitmask — why the row exists at all:
//!
//! ```text
//! 1  FIRST           the node's first sample
//! 2  OVER_THRESHOLD  an attributable movement; the row is real data
//! 4  ANCHOR          kept so the crossing after it reads correctly
//! 8  LATEST          the newest built frame, i.e. the node's current value
//! ```
//!
//! They are a *set*, not an enum: `6` is a crossing that is also the anchor for
//! the crossing after it, which is what a landing diff stack produces. A row
//! with no `OVER_THRESHOLD` bit is not a measured movement, and a UI offering
//! per-diff selection should say so.
//!
//! Slot `3` is `1` when this sample's **step is attributable** — when the
//! previous sample in the series sits at the immediately preceding built frame,
//! so the difference between them is the work of exactly one diff. `0` on the
//! first sample of a series and on the first sample after a gap, where the
//! difference spans an unknown region and must not be drawn as if it did not.
//!
//! Both ride the same column-wise delta encoder as everything else, so a series
//! whose shape barely changes costs about one character per sample — far less
//! than parallel arrays of JSON booleans.
//!
//! # Encoding rules
//!
//! Each of the `stride` columns is encoded independently:
//!
//! - A column's **first non-null value is absolute**; every later non-null
//!   value is the difference from that column's previous non-null value.
//! - `null` means the node had no value for that metric at that sample, and
//!   **does not advance the column's baseline** — the next non-null value is
//!   still measured against the last real one. A chunk whose metric slots are
//!   *all* null is history's record that the node was absent from that frame
//!   entirely: a real event, not a gap in the data.
//! - The four header slots are never null.
//!
//! [`decode_series`] is the reference decoder; the frontend mirrors it.
//!
//! # Trade-offs worth knowing
//!
//! **Floats don't add back exactly.** `a + (b - a) == b` is not guaranteed in
//! IEEE-754, so a naive encoder's error would compound over a long series. The
//! encoder therefore deltas against the *running reconstruction* — the value
//! the decoder will actually hold — rather than against the raw input. Error
//! is then bounded at one rounding step per sample instead of accumulating,
//! and integral metrics (the overwhelming majority) stay bit-exact.
//!
//! **Timestamps and ids ride along per node** rather than in one shared frame
//! table indexed by every sample. The table would be smaller in principle, but
//! threshold filtering means each node keeps its *own* sparse set of frames,
//! so the index column has to be sent per sample regardless — and a
//! delta-encoded `(timestamp, graph_id)` pair costs about what that index
//! costs, without the indirection or a second dictionary to hold in memory.
//!
//! **`f64` carries the integer columns.** Unix seconds and `graph_id` are
//! exact up to 2^53, which they will not reach. Keeping every slot one numeric
//! type is what lets the payload be a single flat array.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_db::HistorySeriesRow;
use unigraph_db::graph_history::Reasons;
use unigraph_rpc::RpcExec;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::Timestamp;
use unigraph_storage_core::TimestampBounds;

use crate::Unigraph;

/// Slots that precede the metric values in every chunk: timestamp, `graph_id`,
/// the reasons bitmask, then whether this sample's step is attributable.
pub const SAMPLE_HEADER_LEN: usize = 4;

const TIMESTAMP_COLUMN: usize = 0;
const GRAPH_ID_COLUMN: usize = 1;
const REASONS_COLUMN: usize = 2;
const ATTRIBUTABLE_COLUMN: usize = 3;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetHistoryInput {
    pub timeline_id: TimelineID,
    /// Nodes to read. Must be non-empty — history is far too large to return
    /// a whole timeline's worth unfiltered.
    pub node_names: Vec<String>,
    /// Inclusive lower bound on sample timestamp, RFC3339 (e.g. `2026-08-05T16:00:00Z`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_start: Option<String>,
    /// Inclusive upper bound on sample timestamp, RFC3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetHistoryOutput {
    /// Metric names in the order every sample chunk's value slots follow the
    /// four header slots. Also fixes the chunk stride at `4 + metrics.len()`.
    pub metrics: Vec<String>,
    /// One entry per requested node, sorted by name. A node with no recorded
    /// history still gets an entry, with an empty stream.
    pub series: Vec<NodeHistory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct NodeHistory {
    pub node_name: String,
    /// The node's whole series, delta-encoded and flattened into
    /// `4 + metrics.len()`-sized chunks in ascending time order. See the module
    /// docs for the layout, and [`decode_series`] for how to read it.
    ///
    /// Not indexable on its own: slot `n` means nothing without the stride, and
    /// no slot after a column's first is an absolute value.
    pub deltas: Vec<Option<f64>>,
}

/// One chunk of [`NodeHistory::deltas`], put back together.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSample {
    /// Unix seconds.
    pub timestamp: i64,
    pub graph_id: i64,
    /// Why the row exists. See the module docs for what each flag means.
    pub reasons: Reasons,
    /// Is the difference from the previous sample in this series the work of
    /// exactly one diff? See the module docs.
    pub attributable: bool,
    /// Values aligned with [`GetHistoryOutput::metrics`]. `None` where the
    /// node had no value for that metric; all-`None` means the node was
    /// absent from the frame.
    pub values: Vec<Option<f64>>,
}

// ── Handler ──────────────────────────────────────────────────

impl RpcExec<Unigraph> for GetHistoryInput {
    type Output = GetHistoryOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<GetHistoryOutput> {
        anyhow::ensure!(
            !self.node_names.is_empty(),
            "node_names must not be empty — history reads are scoped to specific nodes"
        );

        let bounds = to_timestamp_bounds(
            self.timestamp_start.as_deref(),
            self.timestamp_end.as_deref(),
        )?;
        let series = ctx
            .db
            .graph_history
            .series_many(&self.timeline_id, &self.node_names, &bounds, task)
            .await?;

        Ok(to_output(series))
    }
}

// ── Decoding ─────────────────────────────────────────────────

/// Rebuild a node's samples from its delta stream.
///
/// The reference implementation of the format the frontend consumes — kept
/// here, next to the encoder, so the two can't drift apart unnoticed.
pub fn decode_series(metrics: &[String], node: &NodeHistory) -> Result<Vec<DecodedSample>> {
    let stride = SAMPLE_HEADER_LEN + metrics.len();
    anyhow::ensure!(
        node.deltas.len().is_multiple_of(stride),
        "history stream for '{}' holds {} values, which is not a whole number of {stride}-value samples",
        node.node_name,
        node.deltas.len()
    );

    let mut running = vec![None; stride];
    node.deltas
        .chunks_exact(stride)
        .map(|chunk| decode_sample(&node.node_name, chunk, &mut running))
        .collect()
}

/// Undo one chunk, advancing `running` for every column it carries a value for.
fn decode_sample(
    node_name: &str,
    chunk: &[Option<f64>],
    running: &mut [Option<f64>],
) -> Result<DecodedSample> {
    let mut header = chunk
        .iter()
        .enumerate()
        .map(|(column, delta)| {
            let value = running[column].unwrap_or(0.0) + (*delta)?;
            running[column] = Some(value);
            Some(value)
        })
        .collect::<Vec<_>>();

    let values = header.split_off(SAMPLE_HEADER_LEN);
    Ok(DecodedSample {
        timestamp: require_header(node_name, header[TIMESTAMP_COLUMN], "timestamp")?,
        graph_id: require_header(node_name, header[GRAPH_ID_COLUMN], "graph_id")?,
        reasons: decode_reasons(node_name, header[REASONS_COLUMN])?,
        attributable: require_header(node_name, header[ATTRIBUTABLE_COLUMN], "attributable")? != 0,
        values,
    })
}

/// Unknown bits are kept rather than rejected: a stream written by a newer
/// server carries reasons this build cannot name, and dropping them would make
/// a row that is genuinely justified look collapsed.
fn decode_reasons(node_name: &str, slot: Option<f64>) -> Result<Reasons> {
    let bits = require_header(node_name, slot, "reasons")?;
    let bits = u32::try_from(bits)
        .map_err(|_| anyhow::anyhow!("history sample for '{node_name}' has invalid reasons"))?;
    Ok(Reasons::from_bits_retain(bits))
}

fn require_header(node_name: &str, value: Option<f64>, label: &str) -> Result<i64> {
    let value = value.ok_or_else(|| {
        anyhow::anyhow!(
            "history sample for '{node_name}' has a null {label}; header slots are never null"
        )
    })?;
    Ok(value as i64)
}

// ── Encoding ─────────────────────────────────────────────────

/// Fold the per-node rows into the delta-encoded wire shape.
fn to_output(series: BTreeMap<String, Vec<HistorySeriesRow>>) -> GetHistoryOutput {
    let metrics = collect_metric_names(&series);

    let series = series
        .into_iter()
        .map(|(node_name, rows)| to_node_history(node_name, rows, &metrics))
        .collect();

    GetHistoryOutput { metrics, series }
}

/// The union of every metric name appearing in any sample, sorted.
fn collect_metric_names(series: &BTreeMap<String, Vec<HistorySeriesRow>>) -> Vec<String> {
    series
        .values()
        .flatten()
        .flat_map(|row| row.values.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn to_node_history(
    node_name: String,
    mut rows: Vec<HistorySeriesRow>,
    metrics: &[String],
) -> NodeHistory {
    // Deltas are only meaningful in time order, and the X axis they feed has
    // to be monotonic. Storage already returns rows by ascending `graph_id`,
    // so this is a linear scan in practice — it just stops the wire format
    // from silently depending on a query's ORDER BY clause.
    rows.sort_by_key(|row| (row.timestamp, row.graph_id));

    let mut encoder = DeltaEncoder::new(SAMPLE_HEADER_LEN + metrics.len(), rows.len());
    for row in &rows {
        encoder.push(
            TIMESTAMP_COLUMN,
            Some(row.timestamp.to_unix_timestamp() as f64),
        );
        encoder.push(GRAPH_ID_COLUMN, Some(row.graph_id.0 as f64));
        encoder.push(REASONS_COLUMN, Some(f64::from(row.reasons.bits())));
        encoder.push(
            ATTRIBUTABLE_COLUMN,
            Some(f64::from(u8::from(row.attributable))),
        );
        for (offset, metric) in metrics.iter().enumerate() {
            encoder.push(SAMPLE_HEADER_LEN + offset, row.values.get(metric).copied());
        }
    }

    NodeHistory {
        node_name,
        deltas: encoder.finish(),
    }
}

/// Column-wise delta encoder for one node's stream.
///
/// `running` holds what [`decode_series`] will have reconstructed for each
/// column so far, which is deliberately not the same as the last input value:
/// deltaing against the reconstruction keeps float error bounded at one
/// rounding step instead of compounding down the series.
struct DeltaEncoder {
    running: Vec<Option<f64>>,
    out: Vec<Option<f64>>,
}

impl DeltaEncoder {
    fn new(stride: usize, samples: usize) -> Self {
        Self {
            running: vec![None; stride],
            out: Vec::with_capacity(stride * samples),
        }
    }

    /// Append one column's value. `None` is passed through untouched and
    /// leaves the column's baseline where it was.
    fn push(&mut self, column: usize, value: Option<f64>) {
        let Some(value) = value else {
            self.out.push(None);
            return;
        };
        // No baseline yet means this is the column's first value, and
        // `value - 0.0` is exactly the absolute the decoder needs.
        let baseline = self.running[column].unwrap_or(0.0);
        let delta = value - baseline;
        self.running[column] = Some(baseline + delta);
        self.out.push(Some(delta));
    }

    fn finish(self) -> Vec<Option<f64>> {
        self.out
    }
}

// ── Input parsing ────────────────────────────────────────────

fn to_timestamp_bounds(start: Option<&str>, end: Option<&str>) -> Result<TimestampBounds> {
    Ok(TimestampBounds {
        start: start.map(Timestamp::from_rfc3339).transpose()?,
        end: end.map(Timestamp::from_rfc3339).transpose()?,
    })
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use k9::snapshot;
    use unigraph_storage_core::GraphID;

    use super::*;

    /// Fixed, so the snapshot never depends on the wall clock.
    const BASE_TS: i64 = 1_700_000_000;
    /// Frames an hour apart, which is what flattens the timestamp column to a
    /// repeated `3600`.
    const FRAME_SPACING_SECS: i64 = 3600;
    const NODE: &str = "app";

    /// One sample: its metric values, why the row exists, and whether the step
    /// from the sample before it belongs to a single diff.
    struct Sample<'a> {
        values: Vec<(&'a str, f64)>,
        reasons: Reasons,
        attributable: bool,
    }

    /// A case: a name and the series it describes.
    struct Case<'a> {
        name: &'a str,
        samples: Vec<Sample<'a>>,
    }

    fn sample<'a>(values: &[(&'a str, f64)], reasons: Reasons, attributable: bool) -> Sample<'a> {
        Sample {
            values: values.to_vec(),
            reasons,
            attributable,
        }
    }

    #[test]
    fn round_trips_every_shape_of_series() {
        let crossing = Reasons::OVER_THRESHOLD;
        let cases = vec![
            Case {
                name: "steady climb",
                samples: vec![
                    sample(&[("lines", 10.0), ("size", 100.0)], Reasons::FIRST, false),
                    sample(&[("lines", 20.0), ("size", 300.0)], crossing, true),
                    sample(&[("lines", 20.0), ("size", 900.0)], crossing, true),
                ],
            },
            Case {
                name: "metric appears late",
                samples: vec![
                    sample(&[("lines", 10.0)], Reasons::FIRST, false),
                    sample(&[("lines", 12.0), ("size", 5.0)], crossing, true),
                    sample(&[("lines", 12.0), ("size", 6.0)], crossing, true),
                ],
            },
            Case {
                name: "node vanishes mid-series",
                samples: vec![
                    sample(&[("lines", 1.0), ("size", 5.0)], Reasons::FIRST, false),
                    sample(&[], crossing, true),
                    sample(&[("lines", 4.0), ("size", 50.0)], crossing, true),
                ],
            },
            Case {
                name: "fractional metric",
                samples: vec![
                    sample(&[("ratio", 0.1)], Reasons::FIRST, false),
                    sample(&[("ratio", 0.2)], crossing, true),
                    sample(&[("ratio", 0.30000000000000004)], crossing, true),
                    sample(&[("ratio", 0.4)], crossing, true),
                ],
            },
            // The shape compaction leaves behind: a far-apart pair of samples
            // with an anchor pinned right before the second, so the +5 that
            // graph actually contributed is readable instead of the +99 the gap
            // suggests. The anchor's own step is not attributable — the row
            // before it is hundreds of frames back.
            Case {
                name: "anchored threshold crossing",
                samples: vec![
                    sample(&[("size", 1.0)], Reasons::FIRST, false),
                    sample(&[("size", 95.0)], Reasons::ANCHOR, false),
                    sample(&[("size", 100.0)], crossing, true),
                ],
            },
            // A landing diff stack. Every row is a crossing *and* the anchor
            // for the one after it — a combination the old single `anchor` flag
            // could not express, which is why it reported these steps as
            // unattributable and threw them away.
            Case {
                name: "diff stack",
                samples: vec![
                    sample(&[("size", 10.0)], Reasons::FIRST, false),
                    sample(&[("size", 40.0)], crossing | Reasons::ANCHOR, true),
                    sample(&[("size", 70.0)], crossing | Reasons::LATEST, true),
                ],
            },
            // Both sides of a gap keep a row for every node, with no reason of
            // their own, and the step across the hole is explicitly not
            // attributable to anything.
            Case {
                name: "across a gap",
                samples: vec![
                    sample(&[("size", 10.0)], Reasons::FIRST, false),
                    sample(&[("size", 12.0)], Reasons::empty(), true),
                    sample(&[("size", 80.0)], Reasons::empty(), false),
                ],
            },
            Case {
                name: "single sample",
                samples: vec![sample(
                    &[("lines", 7.0)],
                    Reasons::FIRST | Reasons::LATEST,
                    false,
                )],
            },
            Case {
                name: "no samples",
                samples: vec![],
            },
        ];

        let report = cases
            .iter()
            .map(format_case)
            .collect::<Vec<_>>()
            .join("\n\n");

        snapshot!(
            report,
            "
── steady climb
   metrics [lines, size]  stride 6  wire 18 values
   wire    1700000000, 0, 1, 0, 10, 100  |  3600, 1, 1, 1, 10, 200  |  3600, 1, 0, 0, 0, 600
   decode  t+0s     g0  FIRST                  —             lines=10  size=100
           t+3600s  g1  OVER_THRESHOLD         attributable  lines=20  size=300
           t+7200s  g2  OVER_THRESHOLD         attributable  lines=20  size=900
   3 samples, bit-exact

── metric appears late
   metrics [lines, size]  stride 6  wire 18 values
   wire    1700000000, 0, 1, 0, 10, null  |  3600, 1, 1, 1, 2, 5  |  3600, 1, 0, 0, 0, 1
   decode  t+0s     g0  FIRST                  —             lines=10  size=-
           t+3600s  g1  OVER_THRESHOLD         attributable  lines=12  size=5
           t+7200s  g2  OVER_THRESHOLD         attributable  lines=12  size=6
   3 samples, bit-exact

── node vanishes mid-series
   metrics [lines, size]  stride 6  wire 18 values
   wire    1700000000, 0, 1, 0, 1, 5  |  3600, 1, 1, 1, null, null  |  3600, 1, 0, 0, 3, 45
   decode  t+0s     g0  FIRST                  —             lines=1  size=5
           t+3600s  g1  OVER_THRESHOLD         attributable  lines=-  size=-
           t+7200s  g2  OVER_THRESHOLD         attributable  lines=4  size=50
   3 samples, bit-exact

── fractional metric
   metrics [ratio]  stride 5  wire 20 values
   wire    1700000000, 0, 1, 0, 0.1  |  3600, 1, 1, 1, 0.1  |  3600, 1, 0, 0, 0.10000000000000003  |  3600, 1, 0, 0, 0.09999999999999998
   decode  t+0s      g0  FIRST                  —             ratio=0.1
           t+3600s   g1  OVER_THRESHOLD         attributable  ratio=0.2
           t+7200s   g2  OVER_THRESHOLD         attributable  ratio=0.30000000000000004
           t+10800s  g3  OVER_THRESHOLD         attributable  ratio=0.4
   4 samples, bit-exact

── anchored threshold crossing
   metrics [size]  stride 5  wire 15 values
   wire    1700000000, 0, 1, 0, 1  |  3600, 1, 3, 0, 94  |  3600, 1, -2, 1, 5
   decode  t+0s     g0  FIRST                  —             size=1
           t+3600s  g1  ANCHOR                 —             size=95
           t+7200s  g2  OVER_THRESHOLD         attributable  size=100
   3 samples, bit-exact

── diff stack
   metrics [size]  stride 5  wire 15 values
   wire    1700000000, 0, 1, 0, 10  |  3600, 1, 5, 1, 30  |  3600, 1, 4, 0, 30
   decode  t+0s     g0  FIRST                  —             size=10
           t+3600s  g1  OVER_THRESHOLD|ANCHOR  attributable  size=40
           t+7200s  g2  OVER_THRESHOLD|LATEST  attributable  size=70
   3 samples, bit-exact

── across a gap
   metrics [size]  stride 5  wire 15 values
   wire    1700000000, 0, 1, 0, 10  |  3600, 1, -1, 1, 2  |  3600, 1, 0, -1, 68
   decode  t+0s     g0  FIRST                  —             size=10
           t+3600s  g1  -                      attributable  size=12
           t+7200s  g2  -                      —             size=80
   3 samples, bit-exact

── single sample
   metrics [lines]  stride 5  wire 5 values
   wire    1700000000, 0, 9, 0, 7
   decode  t+0s  g0  FIRST|LATEST           —             lines=7
   1 samples, bit-exact

── no samples
   metrics []  stride 4  wire 0 values
   wire    (empty)
   0 samples, bit-exact
"
        );
    }

    #[test]
    fn a_truncated_stream_is_rejected_rather_than_misread() {
        let metrics = vec!["lines".to_owned(), "size".to_owned()];
        let node = NodeHistory {
            node_name: NODE.to_owned(),
            deltas: vec![Some(1.0), Some(2.0), Some(3.0)],
        };

        let Err(err) = decode_series(&metrics, &node) else {
            panic!("3 values cannot be a whole number of 6-value samples");
        };
        assert!(
            format!("{err:#}").contains("not a whole number of 6-value samples"),
            "Error should name the stride mismatch, got: {err:#}"
        );
    }

    // ── Helpers ──────────────────────────────────────────────

    /// Push one case through the real encoder, decode it back, and render both
    /// halves plus the round-trip verdict.
    fn format_case(case: &Case<'_>) -> String {
        let out = to_output(BTreeMap::from([(NODE.to_owned(), to_rows(&case.samples))]));
        let node = &out.series[0];
        let decoded =
            decode_series(&out.metrics, node).expect("the encoder must emit a decodable stream");
        let stride = SAMPLE_HEADER_LEN + out.metrics.len();

        let mut lines = vec![
            format!("── {}", case.name),
            format!(
                "   metrics [{}]  stride {stride}  wire {} values",
                out.metrics.join(", "),
                node.deltas.len()
            ),
            format!("   wire    {}", format_wire(&node.deltas, stride)),
        ];
        lines.extend(format_decoded(&out.metrics, &decoded));
        lines.push(format!(
            "   {}",
            verify(&case.samples, &out.metrics, &decoded)
        ));
        lines.join("\n")
    }

    fn format_wire(deltas: &[Option<f64>], stride: usize) -> String {
        if deltas.is_empty() {
            return "(empty)".to_owned();
        }
        deltas
            .chunks(stride)
            .map(|chunk| chunk.iter().map(format_slot).collect::<Vec<_>>().join(", "))
            .collect::<Vec<_>>()
            .join("  |  ")
    }

    fn format_slot(value: &Option<f64>) -> String {
        value.map_or_else(|| "null".to_owned(), |value| format!("{value}"))
    }

    /// Timestamps are shown as an offset from [`BASE_TS`], and the offset column
    /// is padded so the metric columns line up down the block.
    fn format_decoded(metrics: &[String], decoded: &[DecodedSample]) -> Vec<String> {
        let offsets = decoded
            .iter()
            .map(|sample| format!("t+{}s", sample.timestamp - BASE_TS))
            .collect::<Vec<_>>();
        let width = offsets.iter().map(String::len).max().unwrap_or(0);

        decoded
            .iter()
            .zip(&offsets)
            .enumerate()
            .map(|(index, (sample, offset))| {
                let label = match index {
                    0 => "   decode  ",
                    _ => "           ",
                };
                let values = metrics
                    .iter()
                    .zip(&sample.values)
                    .map(|(name, value)| match value {
                        Some(value) => format!("{name}={value}"),
                        None => format!("{name}=-"),
                    })
                    .collect::<Vec<_>>()
                    .join("  ");
                let step = match sample.attributable {
                    true => "attributable",
                    false => "—",
                };
                format!(
                    "{label}{offset:<width$}  g{}  {:<22} {step:<13} {values}",
                    sample.graph_id,
                    sample.reasons.to_string(),
                )
            })
            .collect()
    }

    /// Assert the decode is faithful, and report how faithful.
    ///
    /// Decoding replays the encoder's own accumulator, so anything the
    /// accumulator can hold exactly — every integral metric, which is nearly
    /// all of them — comes back bit-identical. A fractional metric may land a
    /// rounding step off; the point of deltaing against the reconstruction is
    /// that the gap stays that size instead of compounding down the series, so
    /// the bound below is absolute rather than proportional to sample count.
    fn verify(samples: &[Sample<'_>], metrics: &[String], decoded: &[DecodedSample]) -> String {
        assert_eq!(
            decoded.len(),
            samples.len(),
            "Every sample must survive the round trip"
        );

        let mut worst = 0.0f64;
        for (index, (expected, sample)) in samples.iter().zip(decoded).enumerate() {
            assert_eq!(
                sample.timestamp,
                BASE_TS + index as i64 * FRAME_SPACING_SECS,
                "The timestamp column is integral and must reconstruct exactly"
            );
            assert_eq!(
                sample.graph_id, index as i64,
                "The graph_id column is integral and must reconstruct exactly"
            );
            assert_eq!(
                sample.reasons, expected.reasons,
                "The reasons bitmask decides what a row may be used for, so it \
                 must reconstruct exactly"
            );
            assert_eq!(
                sample.attributable, expected.attributable,
                "Attribution decides whether a step may be blamed on one diff, \
                 so it must reconstruct exactly"
            );

            for (offset, metric) in metrics.iter().enumerate() {
                let want = expected
                    .values
                    .iter()
                    .find(|(name, _)| name == metric)
                    .map(|(_, value)| *value);
                match (sample.values[offset], want) {
                    (Some(got), Some(want)) => worst = worst.max((got - want).abs()),
                    (None, None) => {}
                    (got, want) => panic!(
                        "Presence must survive the round trip: '{metric}' at sample {index} \
                         decoded as {got:?}, expected {want:?}"
                    ),
                }
            }
        }

        assert!(
            worst <= f64::EPSILON,
            "Reconstruction drifted by {worst:e}, which is more than one rounding step"
        );
        match worst == 0.0 {
            true => format!("{} samples, bit-exact", decoded.len()),
            false => format!("{} samples, max drift {worst:e}", decoded.len()),
        }
    }

    fn to_rows(samples: &[Sample<'_>]) -> Vec<HistorySeriesRow> {
        samples
            .iter()
            .enumerate()
            .map(|(index, sample)| HistorySeriesRow {
                graph_id: GraphID(index as i64),
                timestamp: Timestamp::from_unix_timestamp(
                    BASE_TS + index as i64 * FRAME_SPACING_SECS,
                ),
                values: sample
                    .values
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), *value))
                    .collect(),
                reasons: sample.reasons,
                attributable: sample.attributable,
            })
            .collect()
    }
}

# Graph metric history — redesign plan

**Status:** implemented. This document is the design record; the code is the
source of truth. Where they disagree, the code won.

**Decisions taken at implementation time** (Part V "Open" is now closed):

| question | decision |
|---|---|
| `LATEST` (III.7) | **taken.** One row per node at the newest built frame. |
| Does `settle_hours` survive? | **no.** Deleted outright — `MAX_ATTEMPTS` is the only cap left. |
| Store `FIRST`? | **stored**, so `keep ⟺ reasons != 0` stays checkable from one row. |
| `history show` across a gap | **full-width banner**, plus per-row reason tags. |
| Barrier rows at 500k nodes | not guarded. Revisit when a wide timeline is onboarded. |

**Three deviations from this document, all found by the tests:**

1. `NO_DATA` also covers `ingest_state = Failed`. A frame history could not read
   has no values either, so its neighbours get boundary rows like any other gap,
   and it un-gaps itself if a retry succeeds.
2. **Compaction must be told which frames sit after a gap.** The "data frames"
   list makes the two sides of a hole look adjacent, and re-thresholding across
   that invents a crossing for a whole unknown region. `CompactInput::after_gap`
   is what stops it.
3. **Attribution must not cross a gap either.** The read path walks the full
   frame sequence, not just the built frames, for the same reason.

**Scope:** `unigraph_db::graph_history`, `unigraph_db::namespaces::GraphHistory`, the
`graph_history_*` tables in both storage backends, the `GetHistory` RPC, and the
`unigraph history {ingest,compact,show}` CLI.

**Audience:** an agent implementing this. You do not need prior context — Part I describes the
existing system in full.

Written against commit `054f19edeaea`. Line numbers verified at that commit; re-check if files
moved. All paths are relative to `fbcode/`.

---

# Part 0 — What this subsystem is for

A **timeline** is an ordered sequence of graphs, one per landed diff. `www-budget` is the driving
case: nodes are JS budget buckets (`BudgetProjectAdsComet:Duplication`), metrics are tiered
exclusive byte counts (`t1_exc`, `t2_exc`, `t3_exc`).

We want per-node metric history so an engineer can answer **"which diff moved this bucket?"** and
chart it over time.

Recording every node at every frame is unaffordable: `www-budget` produced **24,252 frames in six
days**. So history records a node's value only when it moved by at least a `threshold`.

Everything hard about this subsystem follows from one asymmetry:

> **Keeping a row is reversible. Not writing one is not.**

You can always delete a redundant row later. You can never recover a sample you decided to skip.
Every ambiguous decision must err toward keeping.

And one operational fact:

> **Frames are registered in `graph_id` order but built out of order.**

At any moment the timeline has holes — `Empty` placeholders between built frames. Some fill minutes
later, some fill days later, most never fill at all (the source build failed).

---

# Part I — The system as it exists today

## I.1 Tables

`unigraph_storage_sqlite/src/schema.rs:85-148`. The XDB/MySQL schema mirrors this and is managed
outside the crate; `storage_meta_oss/src/history.rs` holds the queries.

```sql
-- Per-timeline metric-name dictionary. Append-only with stable ids: reordering
-- would make every previously written metric_values blob decode wrong.
CREATE TABLE graph_history_metrics (
    timeline_id  TEXT    NOT NULL,
    metric_id    INTEGER NOT NULL,
    metric_name  TEXT    NOT NULL,
    PRIMARY KEY (timeline_id, metric_id)
);

-- Per-(timeline, graph_id) ingest checkpoint.
CREATE TABLE graph_history_status (
    timeline_id       TEXT    NOT NULL,
    graph_id          INTEGER NOT NULL,
    status            TEXT    NOT NULL,   -- Processed | Omitted | Error | Empty
    attempts          INTEGER NOT NULL DEFAULT 0,
    error_blob_key    TEXT,
    omission_deferred INTEGER NOT NULL DEFAULT 0,
    updated_at        INTEGER NOT NULL,
    PRIMARY KEY (timeline_id, graph_id)
);

CREATE INDEX idx_graph_history_status_deferred
    ON graph_history_status(timeline_id, graph_id) WHERE omission_deferred != 0;

-- Kept per-node samples. All of a node's metrics at one frame are packed into
-- one blob, so row count scales with nodes, not metrics.
CREATE TABLE graph_history_entries (
    timeline_id   TEXT    NOT NULL,
    node_name     TEXT    NOT NULL,
    graph_id      INTEGER NOT NULL,
    timestamp     INTEGER NOT NULL,
    metric_values BLOB    NOT NULL,   -- sorted [(metric_id u32 LE, value f64 LE)]
    deferred      INTEGER NOT NULL DEFAULT 0,
    anchor        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (timeline_id, node_name, graph_id)
);

CREATE INDEX idx_graph_history_entries_graph ON graph_history_entries(timeline_id, graph_id);
CREATE INDEX idx_graph_history_entries_ts    ON graph_history_entries(timeline_id, timestamp);
```

## I.2 The three kinds of row

| kind | flags | meaning |
|---|---|---|
| **crossing** | `deferred=0, anchor=0` | moved ≥ threshold vs the node's **last kept** value. Real data. Usable as a baseline. |
| **anchor** | `anchor=1` | did *not* move enough, kept anyway because it is the built frame immediately before a crossing. Exists so that crossing's step reads as its own diff's contribution rather than all the drift the threshold folded away. **Never a baseline.** Never deleted while the crossing survives. |
| **deferred** | `deferred=1` | ingest couldn't safely decide (an unfilled frame was behind it), so it kept the row unconditionally. **Never a baseline.** Provisional — compaction deletes it. |

Mutually exclusive by construction:
- ingest's `mint_anchors` (`namespaces/graph_history.rs:2113-2115`) skips nodes whose previous
  frame already has a row
- compaction's `compact_series` (`graph_history/compact.rs:75-86`) checks survivors before it can
  flag anything as an anchor

## I.3 Why anchors exist

A stored value is an absolute. If the previous row is 400 frames back, the obvious reading of a
crossing credits one diff with everything the threshold folded away:

```
actual     frame 1: 1   frame 2: 2  …  frame 998: 94   frame 999: 95   frame 1000: 100
kept       frame 1: 1                                                  frame 1000: 100   "+99"?
anchored   frame 1: 1                                  frame 999: 95   frame 1000: 100   "+5"
```

Documented at `namespaces/graph_history.rs:36-60`.

## I.4 Settledness and the frontier

Because a late fill can invalidate a skip decision, the system tracks whether a frame can still
change.

`graph_history/settle.rs:37-62`:

```rust
match frame_type {
    FrameType::Empty        => frame_timestamp < settle_cutoff,   // presumed abandoned after settle_hours
    FrameType::Error        => true,                              // pipeline never retries
    FrameType::Full | Delta => has_terminal_verdict(status),
}

fn has_terminal_verdict(status) -> bool {
    let Some(status) = status else { return false };
    match status {
        Processed | Omitted => true,
        Error               => attempts >= MAX_ATTEMPTS,   // 5
        Empty               => false,
    }
}
```

`settled_frontier` (`namespaces/graph_history.rs:572-610`) walks frames ascending and **`break`s at
the first unsettled one**. That id is the furthest point compaction may touch.

`is_frame_final` (`graph_history.rs:2024-2035`) additionally requires `!omission_deferred`, so
provisionality propagates forward until compaction retires it frame by frame.

## I.5 `history ingest`

CLI: `unigraph_cli/src/history/ingest.rs`. Orchestration: `GraphHistory::ingest`
(`namespaces/graph_history.rs:356-366`, `ingest_frames` at `:684-754`).

```
--timeline-id      timeline to ingest
--lookback-hours   how far back to scan for frames
--threshold        minimum movement to record a row
--settle-hours     age at which an unfilled frame is presumed abandoned (default 48)
--min-id/--max-id  repair a specific range
```

Per frame, in ascending `graph_id`:

1. `frame_action` (`:1996-2011`) — skip `Empty`/`Error`; skip already-`Processed`/`Omitted`.
2. Reconstruct the graph. Chunked replay (`REPLAY_CHUNK_FRAMES = 200`) folds each graph out of the
   previous one — O(L) instead of O(L²) (`extract_chunk`, `:785-820`).
3. `prime_last_kept` (`:1268-1293`) — for nodes not yet seen this run, load their last baseline row
   from the DB.
4. `threshold_frame` (`:2052-2097`) — for each node, `keep_row(last_kept, current, threshold)`:
   - **frame is final** → skip rows below threshold
   - **frame is not final** → write **every** node's row, mark them `deferred`, flag the frame
     `omission_deferred`
5. `mint_anchors` (`:2109-2127`) — for each kept row, write the previous frame's row for that node,
   flagged `anchor`, unless that frame already has a row.
6. Commit rows + checkpoint in one transaction (`commit_processed_frame`, `:1354-1404`).

`keep_row` (`graph_history/threshold.rs:5-53`) compares against `last_kept` and uses `>=`.

## I.6 `history compact`

CLI: `unigraph_cli/src/history/compact.rs`. Orchestration: `GraphHistory::compact` (`:383-404`).

```
--threshold        threshold to (re-)apply
--settle-hours     must match ingest's value
--deferred-only    only revisit frames ingest flagged
--min-id/--max-id/--start/--end   range
```

Both modes clamp the upper bound to `settled_frontier` (`resolve_compact_range`, `:614-650`).

- **`--deferred-only`** (`compact_deferred_frames`, `:1565-1614`) — walk flagged frames ascending,
  re-judge each frame's rows against their current baselines. Scales with flagged frames.
- **full** (`compact_every_node`, `:1749-1807`) — walk every node's whole series via
  `compact_series`. Needed when the threshold itself changed.

`compact_series` (`graph_history/compact.rs:70-112`) is the pure core: walk non-anchor rows,
compute survivors, then mark the built frame immediately before each survivor as an anchor and drop
everything else.

## I.7 Read path

`GraphHistory::series_many` (`:441-475`) → `get_history_series` → SQL at
`unigraph_storage_sqlite/src/history.rs:193-227` and `storage_meta_oss/src/history.rs:352-383`.

Both select `graph_id, timestamp, metric_values, anchor`. **Neither returns `deferred`.**

`GetHistory` RPC (`unigraph_app/src/rpc_req/get_history.rs`) delta-encodes into flat chunks of
stride `3 + metrics.len()`: `[timestamp, graph_id, anchor, …values]`.

## I.8 Storage trait surface

`unigraph_storage_core/src/traits.rs:284-480`:

```
intern_history_metrics            get_history_metric_names
insert_history_entries            get_last_history_entries_before
get_history_series                list_history_node_names
get_history_status                upsert_history_status
get_history_deferred_bounds       clear_history_omission_deferred
list_history_deferred_graph_ids   get_history_entries_at
delete_history_entries_at         set_history_entries_anchor_at
clear_history_entries_deferred    get_history_error_blob_keys
delete_history_entries            delete_history_entries_for_node
delete_history_status             delete_history_metrics
```

---

# Part II — Why it is being replaced

Observed on `www-budget`, 2026-08-12, after a full wipe and re-ingest at `--threshold 2000`.

## II.1 One unfinished frame froze the entire timeline

Frame `1044879946` (2026-08-10T18:43:20) was an `Empty` placeholder when ingest swept past it. It
got stamped `status = Empty`, was skipped, and later filled.

`has_terminal_verdict(Empty) == false` with **no age fallback** → permanently unsettled →
`settled_frontier` stopped one frame earlier, at `1044879927`.

Consequences:

- every frame after it was ingested with `omissible = false` → **every node's row at every frame**
- compaction refused to touch anything at or past it
- the flag propagated forward, so the state was self-sustaining

**Blast radius: the remaining 8,597 frames. ~264,000 rows for 43 nodes.** One node's series held
~4,600 near-identical rows over 36 hours.

Repair required a manual `--lookback-hours 100000` ingest. Round 1 processed 119 previously-skipped
frames and compaction then dropped **75,231 rows**; a second round found 24 more; the third
converged.

## II.2 Two states can never self-heal

`is_frame_settled` applies the `settle_hours` age rule only when the frame's *current* type is
`Empty`. Once it is built, the rule switches to `has_terminal_verdict`, which has no age escape:

| state | self-heals? |
|---|---|
| `FrameType::Empty`, never filled | ✅ ages out at `settle_hours` |
| `FrameType::Error` | ✅ terminal on sight |
| built, `status = Empty` (filled after ingest passed) | ❌ **never** |
| built, `status IS NULL` (never ingested) | ❌ **never** |

Any ingest outage longer than the cron's `--lookback-hours` permanently wedges compaction.

## II.3 The frontier is a hard prefix

`settled_frontier` `break`s at the first unsettled frame. Frames after it are never examined even
when they are contiguous and perfectly judgeable against each other.

## II.4 Compaction structurally trails by `settle_hours`

Not a bug, but at 48h on a pipeline producing ~2,800 built frames/day it means ~240k rows are
permanently live, uncompacted, in the trailing window — and that window is exactly the part of the
chart people look at.

## II.5 `deferred` is invisible to clients

The read path selects `anchor` but not `deferred`. Provisional rows reach the UI looking exactly
like real crossings. Meanwhile the *baseline* query correctly filters them
(`storage_meta_oss/src/history.rs:328-334`: `AND deferred = 0 AND anchor = 0`). The server knows;
the client can't.

## II.6 `anchor` is the wrong bit for a chart

`anchor` answers *"why does this row exist"*. A chart needs *"is the previous row my immediate frame
predecessor"*. When a diff stack lands and consecutive frames each cross the threshold, none is
flagged `anchor` (correctly — none needs anchor protection), so
`format_delta` (`unigraph_cli/src/history/show.rs:103-109`) prints `-` and discards a delta that was
perfectly attributable. The exclusivity is load-bearing for storage but wrong for presentation.

---

# Part III — The new design

## III.1 The one semantic change everything else follows from

**Threshold is measured against the immediately preceding built frame, not against the node's last
kept value.**

```
keep(N)  ⟺  |value(N) − value(N−1)| ≥ threshold        // N−1 = previous BUILT frame
```

This is a deliberate product decision: the series answers **"which diff moved this?"**, not "what
was the size over time". Slow creep — +1 per frame forever — is deliberately **not** tracked; no
single diff is to blame, so no diff is recorded.

**Why this fixes everything:** a decision now depends only on the immediately adjacent frame. A
frame cannot appear *between* two adjacent frames, so **no decision is ever provisional.**

### Accepted consequences

1. **A node that creeps produces almost no rows.** A chart drawing a line through the retained
   points will understate it. See III.7 for the `LATEST` mitigation and the UI implication.
2. **Compaction can only re-threshold upward.** Every crossing stores its anchor, so
   "would this still cross at a higher threshold?" is answerable from rows alone. Lowering the
   threshold needs values that were deleted — a graph refetch. **Document as a non-goal.**
3. **The baseline is a graph fetch, not a table lookup.** Sequential ingest holds the previous
   frame's values in memory (`RunState.prev_frame`); a run starting mid-timeline re-derives them
   with one graph fetch. That machinery already exists (`seed_prev_frame`,
   `namespaces/graph_history.rs:1040-1091`).

## III.2 Rows are kept for *reasons*, OR'd

Not one enum with a priority order — a set of independent reasons. A row exists iff at least one
applies.

```rust
// Node-level: stored in graph_history_entries.reasons
const FIRST:          u32 = 1 << 0;  // the node's first sample in this timeline
const OVER_THRESHOLD: u32 = 1 << 1;  // |v − prev built frame| ≥ threshold
const ANCHOR:         u32 = 1 << 2;  // the next built frame keeps a row for this node
const LATEST:         u32 = 1 << 3;  // optional, see III.7

// Frame-level: stored in graph_history_status.frame_flags
const NO_DATA:    u32 = 1 << 0;  // Empty or Error — a gap
const AFTER_GAP:  u32 = 1 << 1;  // first built frame after a NO_DATA run
const BEFORE_GAP: u32 = 1 << 2;  // last built frame before a NO_DATA run
```

`COLLAPSED` is not a bit. It is `reasons == 0` on a frame with no gap flags — i.e. no row exists.

### The two invariants

```
row exists     ⟺  reasons != 0  OR  frame_flags & (AFTER_GAP | BEFORE_GAP) != 0
is a baseline  ⟺  OVER_THRESHOLD ∈ reasons
```

The second is what lets `ANCHOR` and `OVER_THRESHOLD` coexist. Today `anchor = 1` *means* "not a
baseline", so the two can never both be true; flagging a real crossing as an anchor would remove it
from baseline lookups and cause wrongly-omitted samples downstream. With an explicit baseline
predicate, a row that is both is a baseline **because of** `OVER_THRESHOLD` and protected from
deletion **because of** `ANCHOR`. This is exactly the diff-stack case from II.6.

## III.3 Gaps: `EMPTY ≡ ERROR`, and barriers instead of a frontier

**An `Error` frame is treated identically to an `Empty` one.** Both mean *"we do not have the metric
value at this revision."* The diff landed and changed the code; we simply cannot attribute the
movement. That is exactly as unknown as an unbuilt frame.

Rationale:
- Without boundary rows, a chart draws a straight line across the error and blames the next frame
  for movement that happened inside it.
- An `Error` frame may later be rebuilt into `Full`/`Delta` by the source pipeline.
- One code path for "no data here" instead of two, and no status-row transitions to special-case.
- **Cost is negligible:** 272 Error frames form only 14 runs, and most are adjacent to Empty runs and
  merge. Gaps go from **127 → 139**; boundary rows from 10,922 → 11,954 at 43 nodes across the whole
  6-day timeline.

**Barriers.** The first built frame after a gap (`AFTER_GAP`) and the last built frame before one
(`BEFORE_GAP`) keep a row for every node, unconditionally, regardless of threshold. They bound the
unknown region: the value on each side is known, and the delta across it is explicitly
unattributable.

Barriers are **temporary**. When the gap fills, the reason evaporates and the rows become ordinary
collapse candidates.

**This replaces the settled frontier.** There is no global prefix cap. Compaction runs on each open
segment between barriers, independently.

## III.4 The blast radius of a late fill is exactly 3 frames

When frame K goes `NO_DATA → Ingested`:

1. K−1 loses `BEFORE_GAP`; recompute its remaining reasons.
2. K is judged against K−1.
3. K+1 loses `AFTER_GAP`; re-judge it against K.
4. **K+2 is untouched.** Its baseline is K+1's *value*, and values never change — only reasons do.

No cascade. Compare with today, where the blast radius is the entire remainder of the timeline.

## III.5 What disappears

| removed | why |
|---|---|
| `deferred` column + `HistoryEntryRow::deferred` | no decision is ever provisional |
| `omission_deferred` column | ditto |
| `is_frame_final` + propagation (`graph_history.rs:2024-2035`) | ditto |
| `settled_frontier` as a compaction clamp (`:572-610`) | replaced by per-segment barriers |
| `compact --deferred-only` | there is no deferred work list |
| `settle_hours` as a *correctness* mechanism | barriers are correct whether or not a gap ever fills |
| `get_last_history_entries_before` + its `deferred = 0 AND anchor = 0` filter | baseline is the previous frame, held in memory or refetched |
| `get_history_deferred_bounds`, `list_history_deferred_graph_ids`, `clear_history_omission_deferred`, `clear_history_entries_deferred` | dead |
| both permanent-wedge bugs (II.2) | unrepresentable |
| `HistoryStatus::{Processed, Omitted}` distinction | derivable, and already documented as misleading (`graph_history.rs:57-60`) |

`settle_hours` may survive as a *hygiene* knob ("stop expecting this placeholder to fill, so stop
retrying it"), but nothing depends on it for correctness. Decide explicitly; default to deleting it.

## III.6 New schema

```sql
-- Per-frame. Gap structure is a property of the frame sequence alone, so it
-- lives here and NOT on every node's row: when a gap fills you rewrite 2 rows
-- instead of 2 x node_count.
CREATE TABLE graph_history_status (
    timeline_id    TEXT    NOT NULL,
    graph_id       INTEGER NOT NULL,
    ingest_state   TEXT    NOT NULL,             -- Pending | Ingested | NoData | Failed
    attempts       INTEGER NOT NULL DEFAULT 0,
    error_blob_key TEXT,
    frame_flags    INTEGER NOT NULL DEFAULT 0,   -- NO_DATA | AFTER_GAP | BEFORE_GAP
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (timeline_id, graph_id)
) WITHOUT ROWID;

-- The work list. Unbounded in time: this is what makes an ingest outage
-- recoverable instead of a permanent wedge (fixes II.2).
CREATE INDEX idx_ghs_pending ON graph_history_status(timeline_id, graph_id)
    WHERE ingest_state IN ('Pending', 'NoData');

-- Per (frame, node). `reasons` replaces `deferred` + `anchor`.
CREATE TABLE graph_history_entries (
    timeline_id   TEXT    NOT NULL,
    node_name     TEXT    NOT NULL,
    graph_id      INTEGER NOT NULL,
    timestamp     INTEGER NOT NULL,
    metric_values BLOB    NOT NULL,
    reasons       INTEGER NOT NULL,   -- FIRST | OVER_THRESHOLD | ANCHOR | LATEST
    PRIMARY KEY (timeline_id, node_name, graph_id)
) WITHOUT ROWID;

CREATE INDEX idx_graph_history_entries_graph ON graph_history_entries(timeline_id, graph_id);
CREATE INDEX idx_graph_history_entries_ts    ON graph_history_entries(timeline_id, timestamp);
```
don't create ALTER TABLE. i can later drop/recreate them. just make sure to update the comments.
`graph_history_metrics` is unchanged.

### Naming trap to fix while you are here

Today `HistoryStatus::Error` means *"history ingest failed"* while `FrameType::Error` means *"the
graph build failed"*. Two unrelated things, same word — and once `FrameType::Error ≡ NoData` it gets
worse. Hence `ingest_state: Pending | Ingested | NoData | Failed`, where `Failed` is
history-ingest failure and carries `attempts`.

## III.7 `LATEST` — recommended, not required

With per-previous-frame semantics, a node that creeps produces no rows at all, so the chart's right
edge can be arbitrarily stale. Pin the most recent built frame:

- every node keeps a row at the newest built frame, reason `LATEST`
- when a newer frame arrives, the previous `LATEST` loses that reason and becomes a collapse
  candidate

Cost: one row per node, always. Benefit: the right edge of every chart is the true current value,
and the creep error is bounded to "between the last crossing and now".

**UI implication either way:** if the series is purely per-diff attribution, a line chart of
absolute values asserts something the data does not support — interpolating between retained points
implies gradual change that was never measured. Prefer attributed-delta bars, or a stepped line with
explicit unknown regions across gaps. See `u-fe/HISTORY_CHART_HANDOFF.md`.

## III.8 Algorithms

### Ingest one frame N

```
prev = previous BUILT frame before N        (may not exist; may be across a gap)

if prev is None:
    reasons |= FIRST                                        for every node
else if there is a gap between prev and N:
    reasons stays 0 for all nodes; frame_flags |= AFTER_GAP  (row kept via frame flag)
else:
    for each node:
        if |v_N − v_prev| >= threshold:
            reasons |= OVER_THRESHOLD
            mark prev's row for this node with ANCHOR
        # else: no reason — no row

set ingest_state = Ingested
recompute frame_flags for N (and for prev, whose BEFORE_GAP may now be stale)
```

Values for `prev` come from `RunState.prev_frame` during a sequential run, or one graph refetch at
run start.

### Recompute frame flags

Pure function of the frame-type sequence:

```
NO_DATA(F)    = F.frame_type in {Empty, Error} or ingest_state(F) == NoData
AFTER_GAP(F)  = F is built and the frame before it is NO_DATA
BEFORE_GAP(F) = F is built and the frame after it is NO_DATA
```

Only ever needs recomputing for the immediate neighbours of a frame whose state changed.

### Compact a segment

Segments are the open intervals between barrier frames. Barrier rows are excluded by construction,
so the delete is single-table with no join:

```sql
DELETE FROM graph_history_entries
WHERE timeline_id = ? AND node_name = ?
  AND graph_id > ?  AND graph_id < ?     -- one segment, boundaries exclusive
  AND reasons = 0;
```

Compaction at a *higher* threshold: for each `OVER_THRESHOLD` row, its `ANCHOR` predecessor is
stored, so recompute Δ. If it no longer crosses, clear `OVER_THRESHOLD`; if the anchor then has no
remaining reason, it too becomes deletable.

### Gap fill

See III.4. Touch K−1, K, K+1. Nothing else.

## III.9 Worked example

Values at frames 01–12, threshold 3. This is the target behaviour end-to-end.

```
frame  value   final reasons / flags
-----  -----   --------------------------------------------------
01     10      FIRST                (also AFTER_GAP+BEFORE_GAP while isolated)
02     10      ANCHOR               (Δ0 — kept only to explain 03)
03     15      OVER_THRESHOLD       (Δ5 vs frame 02)
04     15      —  collapsed         (Δ0)
05     15      ANCHOR               (Δ0 — kept to explain 06)
06     20      OVER_THRESHOLD       (Δ5 vs frame 05)
07     20      —  collapsed         (Δ0)
08     21      —  collapsed         (Δ1)
09     22      —  collapsed         (Δ1)
10     23      —  collapsed         (Δ1)
11     24      ANCHOR               (Δ1 — kept to explain 12)
12     29      OVER_THRESHOLD       (Δ5 vs frame 11)
```

Note 08→11 climbing +1/frame with nothing recorded — that is the accepted slow-creep trade (III.1).

Intermediate state, while 01–03 and 06–08 and 12 are still unbuilt:

```
frame  state
-----  -------------------------------------------------
01     NO_DATA
02     NO_DATA
03     NO_DATA
04     AFTER_GAP      row kept for every node, unconditionally
05     BEFORE_GAP     row kept for every node, unconditionally
06     NO_DATA
07     NO_DATA
08     NO_DATA
09     AFTER_GAP      row kept unconditionally
10     — collapsed    (Δ1 vs 09)
11     BEFORE_GAP     row kept unconditionally
12     NO_DATA
```

Watch 04 and 05 across the two states: `AFTER_GAP → collapsed` and `BEFORE_GAP → ANCHOR`. Barriers
are temporary and reasons are recomputed as gaps close.

## III.10 Wire format / RPC changes

`unigraph_app/src/rpc_req/get_history.rs`. Current chunk layout is
`[timestamp, graph_id, anchor, …metrics]`, stride `3 + metrics.len()`.

Change to `[timestamp, graph_id, reasons, attributable, …metrics]`, stride `4 + metrics.len()`:

- **`reasons`** — the raw bitmask. Lets the UI distinguish a crossing from a barrier from an anchor,
  and decide selectability (`OVER_THRESHOLD` present ⇒ selectable).
- **`attributable`** — computed server-side: *is the previous row in this series my immediate built
  frame predecessor?* This is the bit a chart actually needs (II.6). Use `built_frames_in_range`
  (`namespaces/graph_history.rs:1525-1553`) for the adjacency check. Correct for anchor→crossing
  **and** crossing→crossing.

Both ride the existing column-wise delta encoder at ~1 char/sample.

**Also fix while you are there:** the doc comments on `GetHistoryOutput::metrics`
(`get_history.rs:144-146`) and `NodeHistory::deltas` (`:156-158`) still say "two header slots" and
`2 + metrics.len()`. They are already wrong (it is 3) and the staleness has propagated into
`u-fe/__generated__/ts/*.ts`. Fix and re-run `ut typegen`.

## III.11 CLI changes

```
history ingest   --timeline-id --threshold [--lookback-hours] [--min-id/--max-id]
history compact  --timeline-id --threshold [--min-id/--max-id] [--start/--end]
history show     --timeline-id --node-name [--start/--end]
history delete   unchanged
```

- Drop `--settle-hours` from both (or demote to hygiene-only, III.5).
- Drop `--deferred-only`.
- `--lookback-hours` becomes optional and advisory: ingest **always** also sweeps
  `ingest_state IN ('Pending','NoData')` unbounded, via the partial index. That single change makes
  II.2 unrepresentable.
- `history show` should render reasons, e.g. `[CROSSING]`, `[anchor]`, `[gap-edge]`, and print
  `— unknown region —` across gaps instead of a bare `-`.
- `history compact` should report per-segment counts, not one `compacted through` id.

## III.12 Suggested new/changed trait methods

Remove: `get_last_history_entries_before`, `get_history_deferred_bounds`,
`list_history_deferred_graph_ids`, `clear_history_omission_deferred`,
`clear_history_entries_deferred`, `set_history_entries_anchor_at`.

Add:

```rust
/// Frames still needing ingestion, unbounded in time. The wedge fix.
async fn list_pending_history_frames(&mut self, timeline_id, task) -> Result<Vec<GraphID>>;

/// Read/write frame_flags for a set of frames.
async fn get_history_frame_flags(&mut self, timeline_id, graph_ids, task) -> Result<Vec<(GraphID, u32)>>;
async fn set_history_frame_flags(&mut self, timeline_id, rows: &[(GraphID, u32)], task) -> Result<()>;

/// OR / AND-NOT reason bits for specific (graph_id, node) pairs.
async fn update_history_reasons(&mut self, timeline_id, graph_id, node_names, set: u32, clear: u32, task) -> Result<u64>;

/// Delete reasons == 0 rows within an open segment.
async fn delete_collapsed_entries(&mut self, timeline_id, node_name, segment: GraphIDBounds, task) -> Result<u64>;
```

---

# Part IV — Migration plan

Phased so each step is independently landable and testable.

### Phase 1 — Pure logic, no schema

Rewrite `unigraph_db/src/graph_history/` as pure functions over the new model, fully unit-tested,
not yet wired in.

- `reasons.rs` — bitflags, `keep(row)`, `is_baseline(row)`
- `gaps.rs` — `frame_flags(sequence)`, segment enumeration
- `threshold.rs` — rewrite `keep_row` to compare against the previous frame
- `compact.rs` — rewrite `compact_series` to run per segment with barriers

Follow the existing testing convention (`unigraph/oss/CLAUDE.md`): one table of cases → one
`k9::snapshot!`. Port the current fixtures in `compact.rs:114-324` and
`get_history.rs:388-507`. **Add the worked example from III.9 as a snapshot test** — it is the
clearest statement of intent in this document.

### Phase 2 — Schema

- SQLite: add `reasons`, `frame_flags`, `ingest_state`; add the partial index. Dropping the old
  columns needs a table rebuild — do it in the same migration.
- XDB: schema change through the normal process. Queries live in
  `storage_meta_oss/src/history.rs`.
- Update `unigraph_storage_core/src/history.rs` row structs and
  `unigraph_storage_core/src/traits.rs` (III.12).
- `unigraph_storage_tests/` covers both backends — keep them passing in lockstep.

**Backfill:** don't. The old `deferred` rows cannot be classified retroactively (they were kept
*because* no verdict existed), and the threshold semantics changed, so every stored verdict is
stale anyway. **Wipe and re-ingest.** `history delete` + a full `ingest` on `www-budget` took a few
minutes end to end when done on 2026-08-12.

### Phase 3 — Orchestration

Rewrite `unigraph_db/src/namespaces/graph_history.rs` against the new primitives. This file is
~2,200 lines today and roughly a third of it (deferral, settledness, frontier resolution) simply
disappears.

Keep: chunked replay (`extract_chunk`), metric-name interning with its cache
(`refresh_metric_ids`), the `Throughput` progress reporter, chunked status writes, per-frame
transaction boundaries.

### Phase 4 — RPC + CLI

III.10 and III.11. Re-run `ut typegen`.

### Phase 5 — UI

`u-fe/HISTORY_CHART_HANDOFF.md` already specifies the chart. Update it: `anchor` becomes
`reasons`, and `attributable` arrives as a real field instead of the workaround described in its §6.

### Phase 6 — Operations

```
*/15 * * * *  history ingest  --timeline-id www-budget --threshold 2000
*/15 * * * *  history compact --timeline-id www-budget --threshold 2000
```

Alert on:
- frames stuck in `Pending`/`NoData` beyond an expected lag
- `reasons == 0` row count not trending to zero
- growth in barrier count (means the source pipeline's gaps are getting worse)

---

# Part V — Decisions already made, and open questions

## Settled

| decision | rationale |
|---|---|
| Threshold vs **previous built frame**, not last kept | per-diff blame is the product goal; slow creep deliberately untracked (III.1) |
| `Error` ≡ `Empty` — both are gaps | both mean "no value here"; boundary rows preserve attribution honesty; costs 12 extra gaps (III.3) |
| Reasons are an OR'd bitmask, not an enum | lets `ANCHOR` + `OVER_THRESHOLD` coexist (III.2) |
| Gap flags are frame-level, not node-level | 2 row writes per gap fill instead of `2 × node_count` (III.6) |
| No deferral, no settle window for correctness | every decision is final when made (III.1) |
| Wipe and re-ingest rather than backfill | old verdicts are stale under the new semantics |

## Open

1. **`LATEST` (III.7)** — take it or accept a stale right edge on charts?
2. **Does `settle_hours` survive as a hygiene knob**, or is retrying a `NoData` frame forever
   acceptable given the partial index makes it cheap?
3. **Barrier rows across a permanent gap live forever.** At 43 nodes that's ~12k rows and fine. At
   500k nodes it's 139 × 2 × 500k. Does a node-count guard or a gap-merging rule matter for any
   timeline you plan to onboard?
4. **`history show` rendering** across gaps — how loud should "unknown region" be?
5. **Do you want `FIRST` stored**, or derived from `MIN(graph_id)` per node? Stored keeps
   `keep ⟺ reasons != 0` self-contained; derived saves a bit.

---

# Appendix A — File index

## Core logic (rewrite)
| path | role |
|---|---|
| `unigraph/oss/u-be/unigraph_db/src/graph_history.rs` | module doc + constants (`MAX_ATTEMPTS`, `DEFAULT_SETTLE_HOURS`) |
| `unigraph/oss/u-be/unigraph_db/src/graph_history/threshold.rs` | `keep_row` — change the baseline |
| `unigraph/oss/u-be/unigraph_db/src/graph_history/compact.rs` | `compact_series` — segment + barriers |
| `unigraph/oss/u-be/unigraph_db/src/graph_history/settle.rs` | **delete**, replace with `gaps.rs` |
| `unigraph/oss/u-be/unigraph_db/src/graph_history/status.rs` | `HistoryStatus` → `IngestState` |
| `unigraph/oss/u-be/unigraph_db/src/graph_history/pack.rs` | value blob encoding — unchanged |

## Orchestration (rewrite)
| path | role |
|---|---|
| `unigraph/oss/u-be/unigraph_db/src/namespaces/graph_history.rs` | ingest / compact / delete / series; ~2,200 lines, ~⅓ disappears |

## Storage (schema + queries)
| path | role |
|---|---|
| `unigraph/oss/u-be/unigraph_storage_core/src/history.rs` | row structs |
| `unigraph/oss/u-be/unigraph_storage_core/src/traits.rs:284-480` | trait surface |
| `unigraph/oss/u-be/unigraph_storage_sqlite/src/schema.rs:85-148` | SQLite DDL |
| `unigraph/oss/u-be/unigraph_storage_sqlite/src/history.rs` | SQLite queries |
| `unigraph/meta/storage_meta_oss/src/history.rs` | XDB/MySQL queries |
| `unigraph/oss/u-be/unigraph_storage_tests/src/tests/graph_history.rs` | cross-backend tests |

## RPC + CLI + UI
| path | role |
|---|---|
| `unigraph/oss/u-be/unigraph_app/src/rpc_req/get_history.rs` | wire format, encoder/decoder, snapshot tests |
| `unigraph/oss/u-be/unigraph_cli/src/history/{ingest,compact,show,delete}.rs` | CLI |
| `unigraph/oss/u-fe/HISTORY_CHART_HANDOFF.md` | chart spec — update after Phase 4 |
| `unigraph/oss/u-fe/__generated__/ts/{GetHistoryInput,GetHistoryOutput,NodeHistory}.ts` | regenerate with `ut typegen` |

---

# Appendix B — Glossary

| term | meaning |
|---|---|
| **frame** | one graph in a timeline, keyed by `graph_id` (a diff number). Type `Full`, `Delta`, `Empty`, or `Error`. |
| **built frame** | `Full` or `Delta` — a frame that actually carries metrics. |
| **gap** | a run of frames with no data (`Empty` or `Error`). |
| **barrier** | the built frame on either side of a gap. Keeps a row for every node unconditionally. Temporary — released when the gap fills. |
| **crossing** | a row with `OVER_THRESHOLD`. Real movement attributable to one diff. |
| **anchor** | a row kept only so the *next* row's delta is readable. |
| **collapsed** | `reasons == 0` — no row. |
| **baseline** | the value a threshold decision is measured against. New design: the previous built frame's value. |
| **deferred** | *old design only.* A provisional row written when no verdict could be trusted. Deleted by this redesign. |
| **settled frontier** | *old design only.* The furthest point compaction was allowed to reach. Deleted by this redesign. |

---

# Appendix C — Evidence

Measured on `www-budget`, 2026-08-12, via
`buck2 run @//mode/opt //unigraph/meta:unigraph_cli -- oss …` from `fbcode/`.

```
frames (6 days)        24,252   = 16,523 Delta + 6,741 Empty + 716 Full + 272 Error
nodes                      43
built frames/day       ~2,800

frontier pinned at   1044903518   by one Empty frame at 2026-08-10T22:09:36
frames stranded           8,597
rows kept unconditionally ~264,000

gaps (Empty only)           127   median run 12 frames, max 859, 20 single-frame
gaps (Empty or Error)       139   (+12; 272 Error frames form only 14 runs)
barrier rows @ 43 nodes  11,954   vs 264,000 stranded today — 22x cheaper

repair loop (ingest+compact x4):
  round 1  processed=119  ->  dropped 75,231 rows
  round 2  processed=24   ->  dropped 0
  round 3  processed=0    ->  converged
```

Reproduce:

```bash
cd fbcode
buck2 run @//mode/opt //unigraph/meta:unigraph_cli -- \
  oss timelines frames www-budget --json > /tmp/frames.json
buck2 run @//mode/opt //unigraph/meta:unigraph_cli -- \
  oss history show --timeline-id www-budget --node-name 'BudgetProjectAdsComet:Duplication'
buck2 run @//mode/opt //unigraph/meta:unigraph_cli -- \
  oss history compact --timeline-id www-budget --threshold 2000   # prints the frontier
```

// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Structural diff of two JSON values, flattened into a row list a virtualizer
 * can render side by side.
 *
 * Sibling of `tvc_diff/TvcDiffModel`, which does the same job for the fixed
 * shape of a `TraversalConfig`. This one takes any two JSON values, so it is
 * the thing to reach for when you need a diff and do not have a schema.
 *
 * ## Why not a text diff
 *
 * Line-diffing two pretty-printed blobs means an LCS over lines, and lines are
 * the wrong unit for JSON. Inserting one key mid-object desyncs every hunk
 * below it. Reordering an array reports every element as changed, because the
 * trailing comma moves with the element rather than staying on its line.
 * Matching by key and by element value is O(N+M) in the common case and cannot
 * desync.
 *
 * That is also why the rendered lines carry **no trailing commas**. This is a
 * diff view, not a serializer; a comma that appears only because a sibling was
 * appended is a change the reader has to learn to ignore.
 *
 * ## Why this is fast enough to render
 *
 * A node's JSON runs to thousands of lines and nearly all of it is identical
 * between two versions of a graph, so the row list must never be O(lines).
 * Unchanged runs collapse to a single `gap` row carrying an offset and a
 * length, expanded on demand via [`expandJsonGap`].
 *
 *     "edges_dynamic": {   ── sticky: an ancestor of a change
 *       ⋯ 812              ── ONE gap row, not 812 line rows
 *       "rc:gk": {         ── sticky
 *         -/+              ── the only rows proportional to real changes
 *       }
 *     }
 *
 * ## Sticky ancestors
 *
 * A plain run-collapse would swallow the `{` lines above a change and leave a
 * hunk with no visible path — the JSON equivalent of a diff hunk with no
 * `@@ func @@` header. Every container that encloses a change is marked sticky
 * during the walk and never collapses, so the path to a change is always on
 * screen.
 */

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export type LineTone = "context" | "added" | "removed" | "changed";

export interface JsonLine {
  indent: number;
  text: string;
}

export interface LineRow {
  kind: "line";
  /// Position in [`JsonDiff.lines`]. Stable identity for React keys, and what
  /// gap offsets are expressed in.
  index: number;
  tone: LineTone;
  /// Enclosing container's line index, for reconstructing the path of a search
  /// hit without rescanning.
  parent: number | null;
  /// Encloses a change, so it is never collapsed into a gap.
  sticky: boolean;
  left: JsonLine | null;
  right: JsonLine | null;
}

/// A collapsed run of unchanged lines. `start`/`len` index into
/// [`JsonDiff.lines`] — the rows themselves are never re-materialized, because
/// they already exist.
export interface GapRow {
  kind: "gap";
  start: number;
  len: number;
}

export interface TruncatedRow {
  kind: "truncated";
  shown: number;
}

export type JsonDiffRow = LineRow | GapRow | TruncatedRow;

export interface JsonDiffCounts {
  added: number;
  removed: number;
  changed: number;
}

export interface JsonDiff {
  /// Every aligned line, uncollapsed.
  lines: readonly LineRow[];
  /// The collapsed view of `lines`.
  rows: readonly JsonDiffRow[];
  counts: JsonDiffCounts;
  /// Hit [`JsonDiffOpts.maxLines`] and stopped early.
  truncated: boolean;
}

export interface JsonDiffOpts {
  /// Unchanged lines kept either side of a hunk, as in `diff -U`.
  contextRows: number;
  /// Hard cap on emitted lines. A value with a million leaves is not something
  /// anyone reads line by line, and the view must stay responsive.
  maxLines: number;
  /// Cell cap for the array LCS. Past this, arrays align by index.
  maxArrayLcsCells: number;
}

export const DEFAULT_JSON_DIFF_OPTS: JsonDiffOpts = {
  contextRows: 3,
  maxLines: 20000,
  maxArrayLcsCells: 250_000,
};

export function buildJsonDiff(
  left: unknown,
  right: unknown,
  opts: JsonDiffOpts = DEFAULT_JSON_DIFF_OPTS,
): JsonDiff {
  const ctx: BuildCtx = {
    lines: [],
    ancestors: [],
    counts: { added: 0, removed: 0, changed: 0 },
    opts,
    truncated: false,
  };

  walk(ctx, null, 0, { has: true, value: left }, { has: true, value: right });

  const rows: JsonDiffRow[] = layoutRows(ctx.lines, opts);
  if (ctx.truncated) {
    rows.push({ kind: "truncated", shown: ctx.lines.length });
  }

  return {
    lines: ctx.lines,
    rows,
    counts: ctx.counts,
    truncated: ctx.truncated,
  };
}

/// Replacement rows for `gap` — the caller splices these in where it sat.
export function expandJsonGap(
  diff: JsonDiff,
  gap: GapRow,
  mode: "up" | "down" | "all",
  step = 20,
): JsonDiffRow[] {
  const end = gap.start + gap.len;
  const slice = (from: number, to: number) => diff.lines.slice(from, to);

  if (mode === "all" || gap.len <= step) {
    return slice(gap.start, end);
  }
  if (mode === "down") {
    return [
      ...slice(gap.start, gap.start + step),
      { ...gap, start: gap.start + step, len: gap.len - step },
    ];
  }
  return [{ ...gap, len: gap.len - step }, ...slice(end - step, end)];
}

/// Lines matching `query`, searched against every line rather than the
/// collapsed row list.
///
/// Searching `rows` would only ever match what is already expanded — every
/// line inside a gap would be invisible to search, which is exactly the content
/// the reader cannot see and most needs to find.
///
/// Each hit is shown with its enclosing containers so the path is readable.
export function searchJsonDiff(
  diff: JsonDiff,
  query: string,
  limit = 500,
): JsonDiffRow[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...diff.rows];

  const keep = new Set<number>();
  let matched = 0;

  for (const line of diff.lines) {
    if (matched >= limit) break;
    if (!lineMatches(needle, line)) continue;
    matched++;
    for (let at: number | null = line.index; at != null; ) {
      if (keep.has(at)) break;
      keep.add(at);
      at = diff.lines[at]?.parent ?? null;
    }
  }

  return [...keep]
    .sort((a, b) => a - b)
    .map((index) => diff.lines[index])
    .filter((line): line is LineRow => line != null);
}

/// Index of the next row after `from` that is a real change, or `null`. Backs
/// the jump-to-next-change control.
export function findNextJsonChange(
  rows: readonly JsonDiffRow[],
  from: number,
  direction: 1 | -1,
): number | null {
  for (let i = from + direction; i >= 0 && i < rows.length; i += direction) {
    const row = rows[i];
    if (row?.kind === "line" && row.tone !== "context") return i;
  }
  return null;
}

/// Pretty-printed JSON with object keys sorted, matching how the diff renders
/// each side. For callers that want one value on its own — a single-sided view
/// should not reorder its keys relative to the diff.
export function formatJson(value: unknown, indent = 2): string {
  return JSON.stringify(sortKeysDeep(value), null, indent);
}

function sortKeysDeep(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortKeysDeep);
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      sortedKeys(record).map((key) => [key, sortKeysDeep(record[key])]),
    );
  }
  return value;
}

export function jsonDiffRowKey(row: JsonDiffRow): string {
  switch (row.kind) {
    case "line":
      return `l:${row.index}`;
    case "gap":
      return `g:${row.start}:${row.len}`;
    case "truncated":
      return "t";
  }
}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

interface Slot {
  has: boolean;
  value: unknown;
}

interface BuildCtx {
  lines: LineRow[];
  ancestors: number[];
  counts: JsonDiffCounts;
  opts: JsonDiffOpts;
  truncated: boolean;
}

const ABSENT: Slot = { has: false, value: undefined };

function walk(
  ctx: BuildCtx,
  key: string | null,
  indent: number,
  left: Slot,
  right: Slot,
): void {
  if (!left.has && !right.has) return;

  const leftKind = containerKind(left);
  const rightKind = containerKind(right);

  if (left.has && right.has && leftKind != null && leftKind === rightKind) {
    walkContainer(ctx, key, indent, leftKind, left.value, right.value);
    return;
  }

  if (left.has && right.has && leftKind == null && rightKind == null) {
    walkLeaf(ctx, key, indent, left.value, right.value);
    return;
  }

  // Present on one side only, or the same key holds different shapes. Render
  // each side's whole value on its own — pretending an object and a number are
  // "the same line, changed" would hide everything inside the object.
  if (left.has) oneSided(ctx, key, indent, left.value, "removed");
  if (right.has) oneSided(ctx, key, indent, right.value, "added");
}

function walkLeaf(
  ctx: BuildCtx,
  key: string | null,
  indent: number,
  left: unknown,
  right: unknown,
): void {
  const leftText = keyPrefix(key) + stringifyLeaf(left);
  const rightText = keyPrefix(key) + stringifyLeaf(right);

  emit(ctx, {
    tone: leftText === rightText ? "context" : "changed",
    left: { indent, text: leftText },
    right: { indent, text: rightText },
  });
}

function walkContainer(
  ctx: BuildCtx,
  key: string | null,
  indent: number,
  kind: ContainerKind,
  left: unknown,
  right: unknown,
): void {
  const [open, close] = kind === "object" ? ["{", "}"] : ["[", "]"];

  const openIndex = emit(ctx, {
    tone: "context",
    left: { indent, text: keyPrefix(key) + open },
    right: { indent, text: keyPrefix(key) + open },
  });

  if (openIndex >= 0) ctx.ancestors.push(openIndex);
  if (kind === "object") {
    walkObjectBody(ctx, indent, left, right);
  } else {
    walkArrayBody(ctx, indent, left as unknown[], right as unknown[]);
  }
  if (openIndex >= 0) ctx.ancestors.pop();

  const closeIndex = emit(ctx, {
    tone: "context",
    left: { indent, text: close },
    right: { indent, text: close },
  });

  // The body marked the opening line sticky if it held a change. Pin the
  // closing bracket to match, so a changed object is bounded on screen rather
  // than trailing off into a gap — the run after the last change reaches the
  // end of the document, where there is no following hunk to earn it context.
  const openLine = openIndex >= 0 ? ctx.lines[openIndex] : undefined;
  const closeLine = closeIndex >= 0 ? ctx.lines[closeIndex] : undefined;
  if (openLine?.sticky === true && closeLine != null) {
    closeLine.sticky = true;
  }
}

function walkObjectBody(
  ctx: BuildCtx,
  indent: number,
  left: unknown,
  right: unknown,
): void {
  const l = left as Record<string, unknown>;
  const r = right as Record<string, unknown>;

  for (const key of unionKeys(sortedKeys(l), sortedKeys(r))) {
    walk(ctx, key, indent + 1, slot(l, key), slot(r, key));
  }
}

function walkArrayBody(
  ctx: BuildCtx,
  indent: number,
  left: readonly unknown[],
  right: readonly unknown[],
): void {
  for (const [li, ri] of alignArray(left, right, ctx.opts)) {
    walk(
      ctx,
      null,
      indent + 1,
      li == null ? ABSENT : { has: true, value: left[li] },
      ri == null ? ABSENT : { has: true, value: right[ri] },
    );
  }
}

/// One side's whole value, rendered as a run of single-sided lines.
function oneSided(
  ctx: BuildCtx,
  key: string | null,
  indent: number,
  value: unknown,
  tone: "added" | "removed",
): void {
  for (const line of renderLines(key, indent, value)) {
    emit(ctx, {
      tone,
      left: tone === "removed" ? line : null,
      right: tone === "added" ? line : null,
    });
  }
}

function emit(
  ctx: BuildCtx,
  row: Pick<LineRow, "tone" | "left" | "right">,
): number {
  if (ctx.lines.length >= ctx.opts.maxLines) {
    ctx.truncated = true;
    return -1;
  }

  const index = ctx.lines.length;
  ctx.lines.push({
    kind: "line",
    index,
    parent: ctx.ancestors[ctx.ancestors.length - 1] ?? null,
    sticky: false,
    ...row,
  });

  if (row.tone !== "context") {
    ctx.counts[row.tone === "changed" ? "changed" : row.tone]++;
    // Keep the path to this change out of every gap.
    for (const ancestor of ctx.ancestors) {
      const line = ctx.lines[ancestor];
      if (line != null) line.sticky = true;
    }
  }

  return index;
}

// ---------------------------------------------------------------------------
// Array alignment
// ---------------------------------------------------------------------------

/// Pairs of `[leftIndex, rightIndex]`, either side `null` where an element
/// exists only in the other.
///
/// Trimming the shared head and tail first is what makes the usual case cheap:
/// these arrays are mostly sorted edge lists, so inserting one entry leaves an
/// LCS over a handful of elements rather than thousands. Past
/// [`JsonDiffOpts.maxArrayLcsCells`] the arrays pair by index, which is wrong
/// for an insertion but bounded, and only reachable when both sides differ
/// across a very large span.
function alignArray(
  left: readonly unknown[],
  right: readonly unknown[],
  opts: JsonDiffOpts,
): Array<[number | null, number | null]> {
  const l = left.map(canonicalJson);
  const r = right.map(canonicalJson);

  let start = 0;
  const shorter = Math.min(l.length, r.length);
  while (start < shorter && l[start] === r[start]) start++;

  let end = 0;
  while (
    end < shorter - start &&
    l[l.length - 1 - end] === r[r.length - 1 - end]
  ) {
    end++;
  }

  const pairs: Array<[number | null, number | null]> = [];
  for (let i = 0; i < start; i++) pairs.push([i, i]);

  const midL = l.slice(start, l.length - end);
  const midR = r.slice(start, r.length - end);
  pairs.push(
    ...alignMiddle(midL, midR, opts).map(
      ([a, b]): [number | null, number | null] => [
        a == null ? null : a + start,
        b == null ? null : b + start,
      ],
    ),
  );

  for (let i = 0; i < end; i++) {
    pairs.push([l.length - end + i, r.length - end + i]);
  }

  return pairs;
}

function alignMiddle(
  a: readonly string[],
  b: readonly string[],
  opts: JsonDiffOpts,
): Array<[number | null, number | null]> {
  const n = a.length;
  const m = b.length;

  if (n === 0)
    return b.map((_, j): [number | null, number | null] => [null, j]);
  if (m === 0)
    return a.map((_, i): [number | null, number | null] => [i, null]);

  if (n * m > opts.maxArrayLcsCells) {
    return Array.from(
      { length: Math.max(n, m) },
      (_, i): [number | null, number | null] => [
        i < n ? i : null,
        i < m ? i : null,
      ],
    );
  }

  const width = m + 1;
  const lcs = new Int32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * width + j] =
        a[i] === b[j]
          ? (lcs[(i + 1) * width + (j + 1)] as number) + 1
          : Math.max(
              lcs[(i + 1) * width + j] as number,
              lcs[i * width + (j + 1)] as number,
            );
    }
  }

  const pairs: Array<[number | null, number | null]> = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      pairs.push([i, j]);
      i++;
      j++;
    } else if (
      (lcs[(i + 1) * width + j] as number) >=
      (lcs[i * width + (j + 1)] as number)
    ) {
      pairs.push([i, null]);
      i++;
    } else {
      pairs.push([null, j]);
      j++;
    }
  }
  while (i < n) pairs.push([i++, null]);
  while (j < m) pairs.push([null, j++]);

  return pairs;
}

// ---------------------------------------------------------------------------
// Row layout
// ---------------------------------------------------------------------------

/// Walks alternating runs of collapsible/uncollapsible lines, collapsing runs
/// long enough to be worth a gap row.
function layoutRows(
  lines: readonly LineRow[],
  opts: JsonDiffOpts,
): JsonDiffRow[] {
  const rows: JsonDiffRow[] = [];
  const collapsible = (i: number): boolean => {
    const line = lines[i];
    return line != null && line.tone === "context" && !line.sticky;
  };

  const n = lines.length;
  let i = 0;

  while (i < n) {
    if (!collapsible(i)) {
      rows.push(lines[i] as LineRow);
      i++;
      continue;
    }

    let j = i;
    while (j < n && collapsible(j)) j++;

    // Context is only useful next to a hunk, so drop it at the edges.
    const head = i > 0 ? opts.contextRows : 0;
    const tail = j < n ? opts.contextRows : 0;
    const len = j - i;

    if (len <= head + tail) {
      for (let k = i; k < j; k++) rows.push(lines[k] as LineRow);
    } else {
      for (let k = i; k < i + head; k++) rows.push(lines[k] as LineRow);
      rows.push({ kind: "gap", start: i + head, len: len - head - tail });
      for (let k = j - tail; k < j; k++) rows.push(lines[k] as LineRow);
    }
    i = j;
  }

  return rows;
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

type ContainerKind = "object" | "array";

function containerKind(slot: Slot): ContainerKind | null {
  if (!slot.has) return null;
  const value = slot.value;
  if (Array.isArray(value)) return "array";
  if (value !== null && typeof value === "object") return "object";
  return null;
}

function slot(record: Record<string, unknown>, key: string): Slot {
  // `Object.hasOwn` is ES2022; this project targets ES2020.
  return Object.prototype.hasOwnProperty.call(record, key)
    ? { has: true, value: record[key] }
    : ABSENT;
}

function keyPrefix(key: string | null): string {
  return key == null ? "" : `${JSON.stringify(key)}: `;
}

function stringifyLeaf(value: unknown): string {
  return JSON.stringify(value) ?? "undefined";
}

/// Pretty-print one value as lines, for the side that has it.
function renderLines(
  key: string | null,
  indent: number,
  value: unknown,
): JsonLine[] {
  const out: JsonLine[] = [];

  const push = (key: string | null, indent: number, value: unknown): void => {
    if (Array.isArray(value)) {
      out.push({ indent, text: `${keyPrefix(key)}[` });
      for (const item of value) push(null, indent + 1, item);
      out.push({ indent, text: "]" });
      return;
    }
    if (value !== null && typeof value === "object") {
      const record = value as Record<string, unknown>;
      out.push({ indent, text: `${keyPrefix(key)}{` });
      for (const k of sortedKeys(record)) push(k, indent + 1, record[k]);
      out.push({ indent, text: "}" });
      return;
    }
    out.push({ indent, text: keyPrefix(key) + stringifyLeaf(value) });
  };

  push(key, indent, value);
  return out;
}

function lineMatches(needle: string, line: LineRow): boolean {
  return (
    line.left?.text.toLowerCase().includes(needle) === true ||
    line.right?.text.toLowerCase().includes(needle) === true
  );
}

/// Object key order is NOT trustworthy. `JSON.parse` preserves insertion order,
/// but the JS spec hoists integer-like keys ("0", "42") to the front in numeric
/// order — so one node named "123" would desync the merge below.
///
/// The order only has to be self-consistent across both sides, so the cheap
/// O(n) check buys the common case and we sort otherwise.
function sortedKeys(record: Record<string, unknown>): string[] {
  const keys = Object.keys(record);
  for (let i = 1; i < keys.length; i++) {
    if ((keys[i - 1] as string) > (keys[i] as string)) return keys.sort();
  }
  return keys;
}

function unionKeys(a: readonly string[], b: readonly string[]): string[] {
  const out: string[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    const x = a[i] as string;
    const y = b[j] as string;
    if (x === y) {
      out.push(x);
      i++;
      j++;
    } else if (x < y) {
      out.push(x);
      i++;
    } else {
      out.push(y);
      j++;
    }
  }
  while (i < a.length) out.push(a[i++] as string);
  while (j < b.length) out.push(b[j++] as string);
  return out;
}

/// Stable JSON, used as the identity of an array element. Keys sorted so two
/// equal objects compare equal regardless of how they were deserialized.
function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value) ?? "null";
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  const record = value as Record<string, unknown>;
  return `{${sortedKeys(record)
    .filter((key) => record[key] !== undefined)
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`)
    .join(",")}}`;
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Keyed structural diff of two `TraversalConfig`s, flattened into a row list a
 * virtualizer can render.
 *
 * ## Why not a text diff
 *
 * A `TraversalConfig` is a fixed set of sections, each a `BTreeMap` keyed by
 * node name / tag / label. Diffing the pretty-printed JSON would mean an
 * O(ND) line LCS, and inserting one key mid-map would desync every hunk below
 * it. Diffing by key is O(N+M) and can't desync.
 *
 * ## Why this is fast enough to render
 *
 * Sections routinely hold >100k entries, so the row list must never be
 * O(entries). Unchanged runs collapse to a single `gap` row that carries only
 * an offset and a length — expanded on demand via [`expandGap`]. A 100k-entry
 * config with 50 changes yields ~200 rows.
 *
 *     force_nodes  ── section row
 *       ⋯ 41,882   ── ONE gap row, not 41,882 entry rows
 *       ctx/ctx    ── contextRows around each hunk
 *       -/+/~      ── entry rows, the only ones proportional to real changes
 *       ⋯ 58,004
 *
 * The one shape collapsing can't help with is a wholly-added section (nothing
 * is unchanged, so nothing collapses); `maxRowsPerSection` caps that case.
 *
 * ## What counts as an entry
 *
 * Two sections nest, and for both of them the *inner* level is where the
 * entries are: `force_edges` is from → to → decision, and `force_dynamic` is
 * type key → per-edge-name override. Both flatten to a composite key so a
 * single `rc:gk` holding a thousand gatekeeper overrides produces a thousand
 * rows rather than one row with a thousand-entry value. Diffing at the type
 * level would report "`rc:gk` changed" and hand you two blobs to compare by
 * eye, which is the failure this whole model exists to avoid.
 */

import type { TraversalConfig } from "../__generated__/ts/TraversalConfig";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export const SECTION_NAMES = [
  "force_nodes",
  "force_edges",
  "force_tagged",
  "label_predicates",
  "force_dynamic",
  "tiered_traversal",
  "messages",
] as const;

export type SectionName = (typeof SECTION_NAMES)[number];

export type EntryStatus = "added" | "removed" | "changed" | "context";

export interface SectionRow {
  kind: "section";
  section: SectionName;
  added: number;
  removed: number;
  changed: number;
}

/// A collapsed run of unchanged entries. `start`/`len` index into
/// [`SectionData.keys`] — the rows themselves are never materialized.
export interface GapRow {
  kind: "gap";
  section: SectionName;
  start: number;
  len: number;
}

export interface TruncatedRow {
  kind: "truncated";
  section: SectionName;
  shown: number;
  total: number;
}

export interface EntryRow {
  kind: "entry";
  section: SectionName;
  /// Unique within the section. Not for display — see `label`.
  key: string;
  label: string;
  status: EntryStatus;
  /// Canonical JSON of the value, or `null` when absent from that side.
  left: string | null;
  right: string | null;
}

export type DiffRow = SectionRow | GapRow | TruncatedRow | EntryRow;

/// Everything needed to materialize a collapsed gap later.
export interface SectionData {
  /// Sorted union of both sides' keys.
  keys: readonly string[];
  left: ReadonlyMap<string, string>;
  right: ReadonlyMap<string, string>;
  label: (key: string) => string;
}

export interface TvcDiff {
  rows: readonly DiffRow[];
  sections: ReadonlyMap<SectionName, SectionData>;
}

export interface DiffOpts {
  /// Unchanged entries kept either side of a hunk, as in `diff -U`.
  contextRows: number;
  /// Cap on entry rows per section. Guards the wholly-added-section case.
  maxRowsPerSection: number;
}

export const DEFAULT_DIFF_OPTS: DiffOpts = {
  contextRows: 3,
  maxRowsPerSection: 2000,
};

export function buildTvcDiff(
  left: TraversalConfig | null,
  right: TraversalConfig,
  opts: DiffOpts = DEFAULT_DIFF_OPTS,
): TvcDiff {
  const sections = new Map<SectionName, SectionData>();
  const rows: DiffRow[] = [];

  for (const section of SECTION_NAMES) {
    const data = readSection(section, left, right);
    if (data.keys.length === 0) continue;
    sections.set(section, data);
    rows.push(...buildSectionRows(section, data, opts));
  }

  return { rows, sections };
}

/// Replacement rows for `gap` — the caller splices these in where it sat.
export function expandGap(
  diff: TvcDiff,
  gap: GapRow,
  mode: "up" | "down" | "all",
  step = 20,
): DiffRow[] {
  const data = diff.sections.get(gap.section);
  if (data == null) return [gap];

  const end = gap.start + gap.len;
  const context = (i: number) => contextRow(gap.section, data, i);

  if (mode === "all" || gap.len <= step) {
    return range(gap.start, end).map(context);
  }
  if (mode === "down") {
    return [
      ...range(gap.start, gap.start + step).map(context),
      { ...gap, start: gap.start + step, len: gap.len - step },
    ];
  }
  return [
    { ...gap, len: gap.len - step },
    ...range(end - step, end).map(context),
  ];
}

/// Rows matching `query`, searched against the FULL section data rather than
/// the row list.
///
/// Searching `rows` would only ever match what is already expanded — every
/// entry inside a collapsed gap would be invisible to search, which is exactly
/// the content the user can't see and most needs to find.
///
/// `limit` caps materialized rows so a one-character query can't rebuild a
/// 100k-row list on every keystroke.
export function searchTvcDiff(
  diff: TvcDiff,
  query: string,
  limit = 500,
): DiffRow[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...diff.rows];

  const rows: DiffRow[] = [];
  let matched = 0;

  for (const [section, data] of diff.sections) {
    const hits: EntryRow[] = [];
    for (let i = 0; i < data.keys.length && matched < limit; i++) {
      const key = data.keys[i] as string;
      const left = data.left.get(key);
      const right = data.right.get(key);
      if (!matches(needle, data.label(key), left, right)) continue;
      hits.push(
        left === right
          ? contextRow(section, data, i)
          : entryRow(section, data, i),
      );
      matched++;
    }
    if (hits.length === 0) continue;

    const header = diff.rows.find(
      (r) => r.kind === "section" && r.section === section,
    );
    if (header != null) rows.push(header);
    rows.push(...hits);
    if (matched >= limit) break;
  }

  return rows;
}

function matches(
  needle: string,
  label: string,
  left: string | undefined,
  right: string | undefined,
): boolean {
  return (
    label.toLowerCase().includes(needle) ||
    left?.toLowerCase().includes(needle) === true ||
    right?.toLowerCase().includes(needle) === true
  );
}

/// Index of the next row after `from` that represents a real change, or `null`.
/// Backs the jump-to-next-change control — the reason a 100k diff is navigable.
export function findNextChange(
  rows: readonly DiffRow[],
  from: number,
  direction: 1 | -1,
): number | null {
  for (let i = from + direction; i >= 0 && i < rows.length; i += direction) {
    const row = rows[i];
    if (row?.kind === "entry" && row.status !== "context") return i;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Section extraction
// ---------------------------------------------------------------------------

/// Separator for the composite keys the two-level sections are flattened with
/// (`force_edges`, `force_dynamic`). A NUL so it can't collide with a node
/// name, a type key or an edge name — including one containing the arrow or
/// the middot the labels are rendered with. It also sorts before every real
/// character, which is what keeps a type's own row above its overrides.
const COMPOSITE_SEP = "\u0000";

function readSection(
  section: SectionName,
  left: TraversalConfig | null,
  right: TraversalConfig,
): SectionData {
  const l = flattenSection(section, left);
  const r = flattenSection(section, right);
  return {
    keys: unionKeys(sortedKeys(l), sortedKeys(r)),
    left: l,
    right: r,
    label: sectionLabel(section),
  };
}

/// One section reduced to `key -> canonical JSON`, whatever its nesting.
function flattenSection(
  section: SectionName,
  tvc: TraversalConfig | null,
): Map<string, string> {
  const out = new Map<string, string>();
  if (tvc == null) return out;

  if (section === "tiered_traversal") {
    const value = tvc.tiered_traversal;
    if (value != null) out.set(section, canonicalJson(value));
    return out;
  }

  if (section === "force_edges") {
    for (const [from, targets] of Object.entries(tvc.force_edges ?? {})) {
      for (const [to, decision] of Object.entries(targets)) {
        out.set(from + COMPOSITE_SEP + to, canonicalJson(decision));
      }
    }
    return out;
  }

  // `force_dynamic` is two levels deep, and the second one is where the
  // entries actually are: a single `rc:gk` holds an override per gatekeeper,
  // thousands of them. Keyed by type alone it is ONE row whose value is a
  // canonical-JSON blob of the whole thing, so flipping one gatekeeper renders
  // as two truncated strings that differ somewhere off the right edge of the
  // screen. Flattened per override, that same flip is one `changed` row and
  // the rest collapse into a gap.
  if (section === "force_dynamic") {
    for (const [typeKey, config] of Object.entries(tvc.force_dynamic ?? {})) {
      // The type's own row carries everything except the overrides. It is
      // emitted even when empty, so a type that exists but configures nothing
      // is still visible.
      out.set(
        typeKey + COMPOSITE_SEP,
        canonicalJson({ default_branches: config.default_branches }),
      );
      for (const [edgeName, override] of Object.entries(
        config.overrides ?? {},
      )) {
        out.set(typeKey + COMPOSITE_SEP + edgeName, canonicalJson(override));
      }
    }
    return out;
  }

  for (const [key, value] of Object.entries(tvc[section] ?? {})) {
    out.set(key, canonicalJson(value));
  }
  return out;
}

function sectionLabel(section: SectionName): (key: string) => string {
  switch (section) {
    case "force_edges":
      return edgeLabel;
    case "force_dynamic":
      return dynamicLabel;
    default:
      return identity;
  }
}

function edgeLabel(key: string): string {
  return key.replace(COMPOSITE_SEP, " → ");
}

/// `rc:gk · some_gatekeeper` for an override, `rc:gk · (type defaults)` for the
/// type's own row.
function dynamicLabel(key: string): string {
  const at = key.indexOf(COMPOSITE_SEP);
  if (at === -1) return key;
  const typeKey = key.slice(0, at);
  const edgeName = key.slice(at + COMPOSITE_SEP.length);
  return edgeName === ""
    ? `${typeKey} · (type defaults)`
    : `${typeKey} · ${edgeName}`;
}

function identity(key: string): string {
  return key;
}

// ---------------------------------------------------------------------------
// Row building
// ---------------------------------------------------------------------------

function buildSectionRows(
  section: SectionName,
  data: SectionData,
  opts: DiffOpts,
): DiffRow[] {
  const { keys, left, right } = data;
  const isChanged: boolean[] = new Array(keys.length);
  let added = 0;
  let removed = 0;
  let changed = 0;

  for (let i = 0; i < keys.length; i++) {
    const key = keys[i] as string;
    const l = left.get(key);
    const r = right.get(key);
    if (l === undefined) added++;
    else if (r === undefined) removed++;
    else if (l !== r) changed++;
    isChanged[i] = l !== r;
  }

  const header: SectionRow = {
    kind: "section",
    section,
    added,
    removed,
    changed,
  };
  const body = layoutRuns(section, data, isChanged, opts);
  return [header, ...truncate(section, body, opts.maxRowsPerSection)];
}

/// Walks alternating runs of changed/unchanged entries, collapsing unchanged
/// runs that are long enough to be worth a gap row.
function layoutRuns(
  section: SectionName,
  data: SectionData,
  isChanged: readonly boolean[],
  opts: DiffOpts,
): DiffRow[] {
  const rows: DiffRow[] = [];
  const n = isChanged.length;
  let i = 0;

  while (i < n) {
    const runIsChanged = isChanged[i] === true;
    let j = i;
    while (j < n && (isChanged[j] === true) === runIsChanged) j++;

    if (runIsChanged) {
      for (let k = i; k < j; k++) rows.push(entryRow(section, data, k));
      i = j;
      continue;
    }

    // Context is only useful next to a hunk, so drop it at the section edges.
    const head = i > 0 ? opts.contextRows : 0;
    const tail = j < n ? opts.contextRows : 0;
    const len = j - i;

    if (len <= head + tail) {
      for (let k = i; k < j; k++) rows.push(contextRow(section, data, k));
    } else {
      for (let k = i; k < i + head; k++)
        rows.push(contextRow(section, data, k));
      rows.push({
        kind: "gap",
        section,
        start: i + head,
        len: len - head - tail,
      });
      for (let k = j - tail; k < j; k++)
        rows.push(contextRow(section, data, k));
    }
    i = j;
  }

  return rows;
}

function truncate(
  section: SectionName,
  rows: readonly DiffRow[],
  max: number,
): DiffRow[] {
  const total = rows.reduce((n, r) => n + (r.kind === "entry" ? 1 : 0), 0);
  if (total <= max) return [...rows];

  const kept: DiffRow[] = [];
  let shown = 0;
  for (const row of rows) {
    if (row.kind === "entry") {
      if (shown === max) break;
      shown++;
    }
    kept.push(row);
  }
  kept.push({ kind: "truncated", section, shown, total });
  return kept;
}

function entryRow(
  section: SectionName,
  data: SectionData,
  index: number,
): EntryRow {
  const key = data.keys[index] as string;
  const left = data.left.get(key) ?? null;
  const right = data.right.get(key) ?? null;
  const status: EntryStatus =
    left === null ? "added" : right === null ? "removed" : "changed";
  return {
    kind: "entry",
    section,
    key,
    label: data.label(key),
    status,
    left,
    right,
  };
}

function contextRow(
  section: SectionName,
  data: SectionData,
  index: number,
): EntryRow {
  const key = data.keys[index] as string;
  const value = data.left.get(key) ?? data.right.get(key) ?? null;
  return {
    kind: "entry",
    section,
    key,
    label: data.label(key),
    status: "context",
    left: value,
    right: value,
  };
}

// ---------------------------------------------------------------------------
// Keys and values
// ---------------------------------------------------------------------------

/// Object key order is NOT trustworthy here. `JSON.parse` preserves insertion
/// order — which for a Rust `BTreeMap` is sorted — but the JS spec hoists
/// integer-like keys ("0", "42") to the front in numeric order. One node named
/// "123" would silently desync the merge below and shift every gap offset.
///
/// The order only has to be self-consistent across both sides, not identical to
/// Rust's, so the cheap O(n) check buys the common case and we sort otherwise.
function sortedKeys(map: ReadonlyMap<string, unknown>): string[] {
  const keys = [...map.keys()];
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

/// Stable JSON: object keys sorted, `undefined` dropped.
///
/// Both are required for correctness, not tidiness. Rust omits `None` fields
/// (`skip_serializing_if`), so a JS-built value carrying an explicit
/// `undefined` must compare equal to one that omits the field — otherwise the
/// view reports changes that don't exist.
///
/// Recursion depth is bounded by the `TraversalConfig` schema (three levels at
/// most), not by graph depth.
function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value) ?? "null";
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, v]) => v !== undefined)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${canonicalJson(v)}`).join(",")}}`;
}

function range(start: number, end: number): number[] {
  const out: number[] = [];
  for (let i = start; i < end; i++) out.push(i);
  return out;
}

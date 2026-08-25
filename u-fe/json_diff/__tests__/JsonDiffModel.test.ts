// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "vitest";
import type { JsonDiff, JsonDiffRow } from "../JsonDiffModel";
import {
  buildJsonDiff,
  DEFAULT_JSON_DIFF_OPTS,
  expandJsonGap,
  findNextJsonChange,
  searchJsonDiff,
} from "../JsonDiffModel";

const SIGN = { added: "+", removed: "-", changed: "~", context: " " } as const;

/// Renders the two panes the way the view lays them out, so a snapshot shows
/// the alignment and not just the data.
function render(rows: readonly JsonDiffRow[]): string {
  return rows
    .map((row) => {
      if (row.kind === "gap") {
        return `${" ".repeat(38)}⋯ ${row.len} unchanged`;
      }
      if (row.kind === "truncated") {
        return `⋯ truncated after ${row.shown}`;
      }
      const cell = (line: { indent: number; text: string } | null) =>
        (line == null ? "" : "  ".repeat(line.indent) + line.text).padEnd(36);
      return `${SIGN[row.tone]} ${cell(row.left)}│ ${cell(row.right)}`.trimEnd();
    })
    .join("\n");
}

test("aligns an object change, added key and removed key", () => {
  const diff = buildJsonDiff(
    { kept: 1, changed: "old", removed: true },
    { added: [1], kept: 1, changed: "new" },
  );

  expect(render(diff.rows)).toMatchInlineSnapshot(`
    "  {                                   │ {
    +                                     │   "added": [
    +                                     │     1
    +                                     │   ]
    ~   "changed": "old"                  │   "changed": "new"
        "kept": 1                         │   "kept": 1
    -   "removed": true                   │
      }                                   │ }"
  `);
  expect(diff.counts).toEqual({ added: 3, removed: 1, changed: 1 });
});

/// The defect that made a line-oriented diff unusable here: an array insert
/// shifts every element, and with commas on the lines every one of them reads
/// as changed. Matching by element value pairs them up instead.
test("an array insert is one added line, not a whole-array rewrite", () => {
  const diff = buildJsonDiff(
    { edges: ["a", "b", "c", "d"] },
    { edges: ["a", "a0", "b", "c", "d"] },
  );

  expect(render(diff.rows)).toMatchInlineSnapshot(`
    "  {                                   │ {
        "edges": [                        │   "edges": [
          "a"                             │     "a"
    +                                     │     "a0"
          "b"                             │     "b"
          "c"                             │     "c"
          "d"                             │     "d"
        ]                                 │   ]
      }                                   │ }"
  `);
  expect(diff.counts).toEqual({ added: 1, removed: 0, changed: 0 });
});

/// A key whose value changes shape is not "one line, changed" — that would
/// hide everything inside the object.
test("a shape change renders both sides in full", () => {
  const diff = buildJsonDiff({ v: 3 }, { v: { nested: true } });

  expect(render(diff.rows)).toMatchInlineSnapshot(`
    "  {                                   │ {
    -   "v": 3                            │
    +                                     │   "v": {
    +                                     │     "nested": true
    +                                     │   }
      }                                   │ }"
  `);
});

/// Key order is an artifact of deserialization, not a change.
test("object key order is not a change; array order is", () => {
  expect(buildJsonDiff({ b: 1, a: 2 }, { a: 2, b: 1 }).counts).toEqual({
    added: 0,
    removed: 0,
    changed: 0,
  });

  expect(buildJsonDiff(["a", "b"], ["b", "a"]).counts).not.toEqual({
    added: 0,
    removed: 0,
    changed: 0,
  });
});

/// The reason a node with thousands of edges is navigable at all — and the
/// reason the path to the change survives the collapse.
test("unchanged runs collapse but the path to a change stays visible", () => {
  const filler = Object.fromEntries(
    Array.from({ length: 40 }, (_, i) => [`k${String(i).padStart(2, "0")}`, i]),
  );
  const diff = buildJsonDiff(
    { deep: { ...filler, target: "old" } },
    { deep: { ...filler, target: "new" } },
  );

  expect(render(diff.rows)).toMatchInlineSnapshot(`
    "  {                                   │ {
        "deep": {                         │   "deep": {
          "k00": 0                        │     "k00": 0
          "k01": 1                        │     "k01": 1
          "k02": 2                        │     "k02": 2
                                          ⋯ 34 unchanged
          "k37": 37                       │     "k37": 37
          "k38": 38                       │     "k38": 38
          "k39": 39                       │     "k39": 39
    ~     "target": "old"                 │     "target": "new"
        }                                 │   }
      }                                   │ }"
  `);

  // `"deep": {` is an ancestor of the change, so it is sticky and never
  // collapsed — without that the hunk would have no visible path.
  const deep = diff.lines.find((l) => l.left?.text.startsWith('"deep"'));
  expect(deep?.sticky).toBe(true);
});

test("a gap expands up, down and all the way", () => {
  const diff = gappedDiff();
  const gap = diff.rows.find((r) => r.kind === "gap");
  if (gap?.kind !== "gap") throw new Error("expected a gap");

  const sizes = (mode: "up" | "down" | "all") =>
    expandJsonGap(diff, gap, mode, 5).map((r) =>
      r.kind === "gap" ? `gap(${r.len})` : "line",
    );

  expect({
    len: gap.len,
    up: summarize(sizes("up")),
    down: summarize(sizes("down")),
    all: summarize(sizes("all")),
  }).toMatchInlineSnapshot(`
    {
      "all": "line x34",
      "down": "line x5, gap(29)",
      "len": 34,
      "up": "gap(29), line x5",
    }
  `);
});

/// Search must read every line, not the collapsed row list — the lines inside
/// a gap are exactly the ones the reader cannot see and most needs to find.
test("search reaches inside gaps and keeps the path", () => {
  const diff = gappedDiff();

  const collapsedTexts = diff.rows
    .filter((r) => r.kind === "line")
    .map((r) => r.left?.text);
  expect(collapsedTexts).not.toContain('"k12": 12');

  expect(render(searchJsonDiff(diff, "k12"))).toMatchInlineSnapshot(`
    "  {                                   │ {
        "deep": {                         │   "deep": {
          "k12": 12                       │     "k12": 12"
  `);
});

test("jump walks to real changes only", () => {
  const diff = gappedDiff();
  const first = findNextJsonChange(diff.rows, -1, 1);
  expect(first).not.toBeNull();
  expect(diff.rows[first as number]).toMatchObject({ tone: "changed" });
  expect(findNextJsonChange(diff.rows, first as number, 1)).toBeNull();
});

/// The line cap is a bound on the view, not a suggestion.
test("an oversized value stops at the line cap", () => {
  const huge = Object.fromEntries(
    Array.from({ length: 500 }, (_, i) => [`k${i}`, i]),
  );
  const diff = buildJsonDiff({}, huge, {
    ...DEFAULT_JSON_DIFF_OPTS,
    maxLines: 50,
  });

  expect(diff.truncated).toBe(true);
  expect(diff.lines.length).toBe(50);
  expect(diff.rows[diff.rows.length - 1]).toEqual({
    kind: "truncated",
    shown: 50,
  });
});

function gappedDiff(): JsonDiff {
  const filler = Object.fromEntries(
    Array.from({ length: 40 }, (_, i) => [`k${String(i).padStart(2, "0")}`, i]),
  );
  return buildJsonDiff(
    { deep: { ...filler, target: "old" } },
    { deep: { ...filler, target: "new" } },
  );
}

function summarize(items: readonly string[]): string {
  const out: string[] = [];
  for (const item of items) {
    const last = out[out.length - 1];
    const match = last?.match(/^line x(\d+)$/);
    if (item === "line" && match != null) {
      out[out.length - 1] = `line x${Number(match[1]) + 1}`;
    } else if (item === "line" && last === "line") {
      out[out.length - 1] = "line x2";
    } else {
      out.push(item);
    }
  }
  return out.join(", ");
}

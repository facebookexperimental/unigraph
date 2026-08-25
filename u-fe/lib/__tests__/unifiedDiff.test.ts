// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "vitest";
import {
  renderDiffText,
  stableStringify,
  type UnifiedDiff,
  unifiedDiff,
  unifiedJsonDiff,
} from "../unifiedDiff";

function render(diff: UnifiedDiff): string {
  switch (diff.t) {
    case "identical":
      return "(identical)";
    case "too_large":
      return `(too large: ${diff.leftLines} vs ${diff.rightLines} lines)`;
    case "diff":
      return renderDiffText(diff.lines);
  }
}

/// One table over the shapes a node comparison can take. Each case is a pair
/// of documents and the diff they produce.
test("unifiedDiff over every shape", () => {
  const cases: Array<[label: string, left: string, right: string]> = [
    ["identical", "a\nb\nc", "a\nb\nc"],
    ["one line replaced", "a\nb\nc", "a\nB\nc"],
    ["line added", "a\nc", "a\nb\nc"],
    ["line removed", "a\nb\nc", "a\nc"],
    ["left empty", "", "a\nb"],
    ["right empty", "a\nb", ""],
    ["all different", "a\nb", "x\ny"],
  ];

  const table = cases
    .map(([label, left, right]) => {
      const body = render(unifiedDiff(left, right))
        .split("\n")
        .map((line) => `    ${line}`)
        .join("\n");
      return `── ${label} ──\n${body}`;
    })
    .join("\n");

  expect(table).toMatchInlineSnapshot(`
    "── identical ──
        (identical)
    ── one line replaced ──
         a
        -b
        +B
         c
    ── line added ──
         a
        +b
         c
    ── line removed ──
         a
        -b
         c
    ── left empty ──
        -
        +a
        +b
    ── right empty ──
        -a
        -b
        +
    ── all different ──
        -a
        -b
        +x
        +y"
  `);
});

/// Long stretches of unchanged lines collapse to a gap marker, so a node with
/// hundreds of identical edges shows only what moved.
test("unchanged runs collapse to a gap", () => {
  const left = ["head", ...range(20), "OLD", ...range(20, 40), "tail"];
  const right = ["head", ...range(20), "NEW", ...range(20, 40), "tail"];

  expect(render(unifiedDiff(left.join("\n"), right.join("\n"))))
    .toMatchInlineSnapshot(`
    "@@ 18 unchanged lines @@
     17
     18
     19
    -OLD
    +NEW
     20
     21
     22
    @@ 18 unchanged lines @@"
  `);
});

/// Key order is an artifact of deserialization, not a change to the node.
test("object key order is not a diff, array order is", () => {
  const reordered = unifiedJsonDiff(
    { b: 1, a: 2, nested: { z: 1, y: 2 } },
    { a: 2, b: 1, nested: { y: 2, z: 1 } },
  );
  expect(reordered.t).toBe("identical");

  expect(render(unifiedJsonDiff({ edges: ["a", "b"] }, { edges: ["b", "a"] })))
    .toMatchInlineSnapshot(`
      " {
         "edges": [
      -    "a",
      -    "b"
      +    "b",
      +    "a"
         ]
       }"
    `);

  expect(stableStringify({ b: 1, a: 2 })).toMatchInlineSnapshot(`
    "{
      "a": 2,
      "b": 1
    }"
  `);
});

/// The quadratic step is bounded. A pair of documents that share no head or
/// tail and are both large declines rather than allocating a huge table.
test("an oversized changed region declines instead of allocating", () => {
  const left = range(1200)
    .map((n) => `L${n}`)
    .join("\n");
  const right = range(1200)
    .map((n) => `R${n}`)
    .join("\n");

  expect(render(unifiedDiff(left, right))).toBe(
    "(too large: 1200 vs 1200 lines)",
  );

  // The same size is fine when the change is localised, because the shared
  // head and tail are trimmed before the LCS runs.
  const shared = range(1200).map((n) => `S${n}`);
  const mutated = [...shared];
  mutated[600] = "CHANGED";
  const localised = unifiedDiff(shared.join("\n"), mutated.join("\n"));
  expect(localised.t).toBe("diff");
});

function range(from: number, to?: number): number[] {
  const [start, end] = to == null ? [0, from] : [from, to];
  return Array.from({ length: end - start }, (_, i) => start + i);
}

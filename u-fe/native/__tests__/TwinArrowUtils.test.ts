// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "vitest";
import type { Arrow } from "../../__generated__/ts/Arrow";
import type { TwinArrow } from "../../__generated__/ts/TwinArrow";
import { skippedNodeCount } from "../TwinArrowUtils";

function arrow(skipped: number): Arrow {
  return {
    tag: undefined,
    dynamic: undefined,
    points_from: 0,
    points_to: 1,
    excluded: false,
    message: undefined,
    skipped,
  };
}

function twinArrow(l: Arrow | undefined, r: Arrow | undefined): TwinArrow {
  return { points_to: 1, points_from: 0, node_diff: 0, l, r };
}

/// One table over every shape a row can take. The one-sided cases are the
/// point: an added or removed node has an edge on a single side, and that is
/// exactly the row a delta view exists to show.
test("skippedNodeCount over every arrow shape", () => {
  const cases: Array<[label: string, twinArrow: TwinArrow, expected: number]> =
    [
      ["both sides, equal", twinArrow(arrow(3), arrow(3)), 3],
      ["both sides, differing", twinArrow(arrow(5), arrow(2)), 2],
      ["both sides, direct edge", twinArrow(arrow(0), arrow(0)), 0],
      ["node removed (right missing)", twinArrow(arrow(3), undefined), 3],
      ["node added (left missing)", twinArrow(undefined, arrow(2)), 2],
      ["neither side", twinArrow(undefined, undefined), 0],
    ];

  const table = cases
    .map(([label, ta, expected]) => {
      const actual = skippedNodeCount(ta);
      expect(actual, label).toBe(expected);
      return `${label.padEnd(30)} l=${String(ta.l?.skipped).padEnd(9)} r=${String(
        ta.r?.skipped,
      ).padEnd(9)} → ${actual}`;
    })
    .join("\n");

  expect(table).toMatchInlineSnapshot(`
    "both sides, equal              l=3         r=3         → 3
    both sides, differing          l=5         r=2         → 2
    both sides, direct edge        l=0         r=0         → 0
    node removed (right missing)   l=3         r=undefined → 3
    node added (left missing)      l=undefined r=2         → 2
    neither side                   l=undefined r=undefined → 0"
  `);
});

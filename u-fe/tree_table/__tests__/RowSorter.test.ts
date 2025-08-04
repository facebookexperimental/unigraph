// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { Arrow } from "@/__generated__/ts/Arrow";
import type { SortOrder } from "@/__generated__/ts/SortOrder";
import { expect, test } from "vitest";
import { type Row, sortRows } from "../TreeTableRows";

function arrow(fromIDX: number, toIDX: number): Arrow {
  return {
    tag: undefined,
    branch: undefined,
    properties: undefined,
    points_from: fromIDX,
    points_to: toIDX,
    excluded: false,
    message: undefined,
  };
}
//    1
//    2
//    * 3
//    * * 40
//    * * * 9
//    * * * * 0
//    * * * 5
//    * * 6
//    * * 7
//    8
const ROW_8: Row = {
  depth: 0,
  expanded: false,
  isCycle: false,
  arrow: arrow(-1, 8),
  transitiveChildrenCount: 0,
  childrenRefs: [],
  parentRowRef: null,
};
const ROW_7: Row = {
  depth: 2,
  expanded: false,
  isCycle: false,
  arrow: arrow(3, 7),
  transitiveChildrenCount: 0,
  childrenRefs: [],
  parentRowRef: null,
};
const ROW_6: Row = {
  depth: 2,
  expanded: false,
  isCycle: false,
  arrow: arrow(3, 6),
  transitiveChildrenCount: 0,
  childrenRefs: [],
  parentRowRef: null,
};
const ROW_5: Row = {
  depth: 3,
  expanded: false,
  isCycle: false,
  arrow: arrow(40, 5),
  transitiveChildrenCount: 0,
  childrenRefs: [],
  parentRowRef: null,
};
const ROW_0: Row = {
  depth: 4,
  expanded: false,
  isCycle: false,
  arrow: arrow(9, 0),
  transitiveChildrenCount: 0,
  childrenRefs: [],
  parentRowRef: null,
};
const ROW_9: Row = {
  depth: 3,
  expanded: false,
  isCycle: false,
  arrow: arrow(40, 9),
  transitiveChildrenCount: 0,
  childrenRefs: [ROW_0],
  parentRowRef: null,
};
const ROW_40: Row = {
  depth: 2,
  expanded: true,
  isCycle: false,
  arrow: arrow(3, 40),
  transitiveChildrenCount: 0,
  childrenRefs: [ROW_9, ROW_5],
  parentRowRef: null,
};
const ROW_3: Row = {
  depth: 1,
  expanded: true,
  isCycle: false,
  arrow: arrow(2, 3),
  transitiveChildrenCount: 0,
  childrenRefs: [ROW_40, ROW_6, ROW_7],
  parentRowRef: null,
};
const ROW_2: Row = {
  depth: 0,
  expanded: true,
  isCycle: false,
  arrow: arrow(-1, 2),
  transitiveChildrenCount: 0,
  childrenRefs: [ROW_3],
  parentRowRef: null,
};

const ROW_1: Row = {
  depth: 0,
  expanded: false,
  isCycle: false,
  arrow: arrow(-1, 1),
  transitiveChildrenCount: 0,
  childrenRefs: [],
  parentRowRef: null,
};

const TEST_ROWS: Row[] = [
  ROW_1,
  ROW_2,
  ROW_3,
  ROW_40,
  ROW_9,
  ROW_0,
  ROW_5,
  ROW_6,
  ROW_7,
  ROW_8,
];

test("printing", () => {
  expect(printRows(TEST_ROWS)).toMatchInlineSnapshot(`
    "
    1              transitiveChildrenCount: 0
    2              transitiveChildrenCount: 0
    * 3            transitiveChildrenCount: 0
    * * 40         transitiveChildrenCount: 0
    * * * 9        transitiveChildrenCount: 0
    * * * * 0      transitiveChildrenCount: 0
    * * * 5        transitiveChildrenCount: 0
    * * 6          transitiveChildrenCount: 0
    * * 7          transitiveChildrenCount: 0
    8              transitiveChildrenCount: 0
    "
  `);
});

test("sorting", () => {
  const ascOne = asc(TEST_ROWS);
  expect(printRows(ascOne)).toMatchInlineSnapshot(`
    "
    1              transitiveChildrenCount: 0
    2              transitiveChildrenCount: 7
    * 3            transitiveChildrenCount: 6
    * * 6          transitiveChildrenCount: 0
    * * 7          transitiveChildrenCount: 0
    * * 40         transitiveChildrenCount: 3
    * * * 5        transitiveChildrenCount: 0
    * * * 9        transitiveChildrenCount: 1
    * * * * 0      transitiveChildrenCount: 0
    8              transitiveChildrenCount: 0
    "
  `);

  const descOne = desc(ascOne);
  expect(printRows(descOne)).toMatchInlineSnapshot(`
    "
    8              transitiveChildrenCount: 0
    2              transitiveChildrenCount: 7
    * 3            transitiveChildrenCount: 6
    * * 40         transitiveChildrenCount: 3
    * * * 9        transitiveChildrenCount: 1
    * * * * 0      transitiveChildrenCount: 0
    * * * 5        transitiveChildrenCount: 0
    * * 7          transitiveChildrenCount: 0
    * * 6          transitiveChildrenCount: 0
    1              transitiveChildrenCount: 0
    "
  `);

  const ascTwo = asc(descOne);
  expect(printRows(ascTwo)).toMatchInlineSnapshot(`
    "
    1              transitiveChildrenCount: 0
    2              transitiveChildrenCount: 7
    * 3            transitiveChildrenCount: 6
    * * 6          transitiveChildrenCount: 0
    * * 7          transitiveChildrenCount: 0
    * * 40         transitiveChildrenCount: 3
    * * * 5        transitiveChildrenCount: 0
    * * * 9        transitiveChildrenCount: 1
    * * * * 0      transitiveChildrenCount: 0
    8              transitiveChildrenCount: 0
    "
  `);

  expect(printRows(ascOne)).toEqual(printRows(ascTwo));

  const descTwo = desc(ascTwo);
  expect(printRows(descTwo)).toMatchInlineSnapshot(`
    "
    8              transitiveChildrenCount: 0
    2              transitiveChildrenCount: 7
    * 3            transitiveChildrenCount: 6
    * * 40         transitiveChildrenCount: 3
    * * * 9        transitiveChildrenCount: 1
    * * * * 0      transitiveChildrenCount: 0
    * * * 5        transitiveChildrenCount: 0
    * * 7          transitiveChildrenCount: 0
    * * 6          transitiveChildrenCount: 0
    1              transitiveChildrenCount: 0
    "
  `);

  expect(printRows(descOne)).toEqual(printRows(descTwo));

  const ascThree = asc(descTwo);
  expect(printRows(ascThree)).toMatchInlineSnapshot(`
    "
    1              transitiveChildrenCount: 0
    2              transitiveChildrenCount: 7
    * 3            transitiveChildrenCount: 6
    * * 6          transitiveChildrenCount: 0
    * * 7          transitiveChildrenCount: 0
    * * 40         transitiveChildrenCount: 3
    * * * 5        transitiveChildrenCount: 0
    * * * 9        transitiveChildrenCount: 1
    * * * * 0      transitiveChildrenCount: 0
    8              transitiveChildrenCount: 0
    "
  `);

  expect(printRows(ascTwo)).toEqual(printRows(ascThree));
});

function printRows(rows: Row[]): string {
  const lines = rows.map(
    (row) =>
      `${"* ".repeat(row.depth)}${row.arrow.points_to}${" ".repeat(15 - row.depth * 2 - row.arrow.points_to.toString().length)}transitiveChildrenCount: ${row.transitiveChildrenCount}`,
  );

  return `\n${lines.join("\n")}\n`;
}

function asc(rows: Row[]): Row[] {
  const sortFn = (a: Row, b: Row) => compare(a, b, "Asc");
  return sortRows(rows, sortFn);
}

function desc(rows: Row[]): Row[] {
  const sortFn = (a: Row, b: Row) => compare(a, b, "Desc");
  return sortRows(rows, sortFn);
}

function compare(a: Row, b: Row, order: SortOrder): 0 | -1 | 1 {
  if (a.arrow.points_to < b.arrow.points_to) {
    return order === "Asc" ? -1 : 1;
  } else if (a.arrow.points_to > b.arrow.points_to) {
    return order === "Asc" ? 1 : -1;
  }
  return 0;
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { ArrowPair } from "../TwinGraph";
import type { NodeIDX } from "../types";
import type { SortState } from "./TreeTable";

export type RowIDX = number;
export type Row = {
  depth: number;
  expanded: boolean;
  isCycle: boolean;
  arrow_pair: ArrowPair;

  parentRowRef: Row | null;
  childrenRefs: Row[];

  // How many rows are between this row and the next row
  // that's NOT a child of this row.
  // This is used to calculate how many rows will be added
  // when this row is expanded. Or how many rows we need to
  // remove when we collapse it.
  //                  Transitive Children Count
  //    A             3    <- B, C, D
  //    * B           0    <- no children
  //    * C           1    <- D
  //    * * D         0    <- no children
  transitiveChildrenCount: number;
};

export type SortFn = (a: Row, b: Row) => -1 | 0 | 1;

// When order of the rows changes, we need to resort them.
// This is kinda not trivial, cause the rows are a tree
// represented as a flat array. Which means that sibling
// nodes can be an uber ride apart from each other with a
// ton of other children nodes in between.
//
// e.g. if the tree is sorted ASC, we can have:
//
//    A       1
//    * B     100
//    * * C   1
//    * D     200
//
// TO sort it DESC we'd need to swap B and D and make it
//    A       1
//    * D     200
//    * B     100
//    * * C   1
//
// In the tree structure it's pretty easy, but in a flat list of
// rows we'd need to make sure node C moves together with B when we
// swap them.
export function sortRows(rows: Row[], sortFn: SortFn): Row[] {
  if (rows.length === 0) {
    return rows;
  }

  const topologicallySorted = [];

  const resultTree = rows.filter((row) => row.depth === 0);
  resultTree.sort(sortFn);
  resultTree.reverse();

  const sortStack = [...resultTree];
  while (sortStack.length > 0) {
    const node = sortStack.pop() as Row;
    node.childrenRefs.sort(sortFn);

    topologicallySorted.push(node);

    for (let i = 0; i < node.childrenRefs.length; i++) {
      const child = node.childrenRefs[i] as Row;
      sortStack.push(child);
    }
  }

  // Topoligically sorted array is a "bottom up" traversal order.
  // This means every next node in this iteration has its children
  // already iterated over. This will allow us to set the offsets
  // in a single pass.
  for (let i = topologicallySorted.length - 1; i >= 0; i--) {
    const node = topologicallySorted[i] as Row;
    const offset = node.childrenRefs.reduce(
      (acc, child) =>
        acc +
        // if a child has other children we nee to add them. Since it's
        // a reverse topoligical order we know that the children
        // already have their offsets set.
        child.transitiveChildrenCount +
        1, // plus the child itself gets one offset
      0,
    );
    node.transitiveChildrenCount = offset;
  }

  // At this point we have a tree where all the children are sorted
  // and the offsets are set. Now we need to flatten it back into
  // a single array of rows. This is a simple DFS.
  const sortedRows = [];
  const sortedStack = [...resultTree];
  while (sortedStack.length > 0) {
    const node = sortedStack.pop() as Row;
    sortedRows.push(node);
    // NOTE: since it's a DFS, which is a LIFO stack, we need to push
    // the children in reverse order to get them in the right order
    // when we pop them. so we'll do i-- instead of i++
    for (let i = node.childrenRefs.length - 1; i >= 0; i--) {
      const child = node.childrenRefs[i] as Row;
      sortedStack.push(child);
    }
  }

  return sortedRows;
}

export function expandRow(
  rows: Row[],
  rowIDX: number,
  arrow_pairs: ArrowPair[],
  sortState: SortState | null,
): void {
  const row = rows[rowIDX];
  if (row == null || row.expanded === true) {
    return;
  }

  const newRows: Row[] = [];
  // iterate over the children and add them to the newRows array
  for (let i = 0; i < arrow_pairs.length; i++) {
    const childArrow = arrow_pairs[i] as ArrowPair;

    const newRow = {
      depth: row.depth + 1,
      expanded: false,
      isCycle: false,
      arrow_pair: childArrow,
      parentRowRef: row,
      childrenRefs: [],
      transitiveChildrenCount: 0,
    };

    // determine if this new row is a cycle by
    // going upt the tree and checking if it already
    // exists in the path.
    let current: Row | null = newRow.parentRowRef;
    while (current != null) {
      if (current.arrow_pair.points_to === newRow.arrow_pair.points_to) {
        newRow.isCycle = true;
        break;
      }
      current = current.parentRowRef;
    }

    newRows.push(newRow);
  }

  // After we expand we need to go up the tree and
  // add the offsets/transitive counts to every row,
  // So when we collapse any of the parents we know
  // exacly how many rows we want to nuke from the list.
  let parent = row.parentRowRef;
  while (parent != null) {
    parent.transitiveChildrenCount += newRows.length;
    parent = parent.parentRowRef;
  }

  if (sortState != null) {
    newRows.sort(sortState.sortFn);
  }

  rows.splice(rowIDX + 1, 0, ...newRows);
  row.expanded = true;
  row.transitiveChildrenCount = newRows.length;
  row.childrenRefs = newRows;
}

export function collapseRow(rows: Row[], rowIDX: number) {
  const row = rows[rowIDX];
  if (row == null || row.expanded === false) {
    return;
  }

  const rowsToDelete = row.transitiveChildrenCount;
  rows.splice(rowIDX + 1, rowsToDelete);

  // After we collapse we need to go up the tree and
  // add the offsets/transitive counts to every row,
  let parent = row.parentRowRef;
  while (parent != null) {
    parent.transitiveChildrenCount -= rowsToDelete;
    parent = parent.parentRowRef;
  }

  row.expanded = false;
  row.childrenRefs = [];
}

/// Try to navigate to the provided path expanding the rows on the way.
/// Return the index of the last row in the path.
/// If the path is invalid, it will expand as much as possible and return
/// null.
export function expandToPath(
  rows: Row[],
  nodeIDXPath: NodeIDX[],
  getArrowPairs: (nodeIDX: NodeIDX) => ArrowPair[],
  sortState: SortState | null,
): RowIDX | null {
  if (nodeIDXPath.length === 0) {
    return null;
  }

  let currentChildrenRefs: Row[] = rows;
  let currentGlobalRowIDX = -1;

  for (let i = 0; i < nodeIDXPath.length; i++) {
    const currentParentRow = rows[currentGlobalRowIDX];
    if (currentParentRow != null) {
      const childrenArrows = getArrowPairs(
        currentParentRow.arrow_pair.points_to,
      );
      expandRow(rows, currentGlobalRowIDX, childrenArrows, sortState);
      currentChildrenRefs = currentParentRow.childrenRefs;
    }

    const nextNodeIDXInPath = nodeIDXPath[i] as NodeIDX;

    const childRowIDX = currentChildrenRefs.findIndex(
      (row) => row.arrow_pair.points_to === nextNodeIDXInPath,
    );

    if (childRowIDX === -1) {
      break;
    }

    currentGlobalRowIDX += childRowIDX + 1;
  }
  return currentGlobalRowIDX === -1 ? null : currentGlobalRowIDX;
}

export function pathToRow(row: Readonly<Row>): NodeIDX[] {
  const path: NodeIDX[] = [];
  let current: Row | null = row;
  while (current != null) {
    path.push(current.arrow_pair.points_to);
    current = current.parentRowRef;
  }
  return path.reverse();
}

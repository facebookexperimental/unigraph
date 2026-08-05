// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "../__generated__/ts/NodeIDX";
import type { SortOrder } from "../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../ArrowUtils";
import { isFlatListEntryPoints } from "../GraphStructureHooks";
import type { ColumnInternal, NumericValueColumnDefinition } from "./columns";
import type { TreeTableGraph } from "./TreeTable";
import {
  collapseRow,
  expandRow,
  expandToPath,
  type Row,
  type SortFn,
  sortRows,
} from "./TreeTableRows";

export class TreeTableCtx {
  rows: Array<Row>;
  columns: ColumnInternal[];
  forceUpdate: () => void;
  sortState: SortState | null;
  treeTableGraph: TreeTableGraph;
  selectedRowIDX: number | null;
  selectedNodeIDXPath: NodeIDX[];
  scrollToIndex: (index: number, options: { align: "center" }) => void;

  constructor(
    columns: ColumnInternal[],
    selectedNodeIDXPath: NodeIDX[],
    treeTableGraph: TreeTableGraph,
  ) {
    this.columns = columns;
    this.rows = [];
    this.forceUpdate = () => {};
    this.sortState = null;
    this.treeTableGraph = treeTableGraph;
    this.selectedRowIDX = null;
    this.scrollToIndex = () => {};
    this.selectedNodeIDXPath = selectedNodeIDXPath;
  }

  updateSortState() {
    this.sortState = null;
    const result: [ColumnInternal, SortOrder] | null = (() => {
      for (const column of this.columns) {
        switch (column.t) {
          case "tree":
          case "numeric_value_column":
            if (column.c.sortable?.order != null) {
              return [column, column.c.sortable.order];
            }
            continue;
          case "non_sortable_column":
            continue;
        }
      }

      return null;
    })();

    if (result == null) {
      return;
    }

    const [sortColumn, sortOrder] = result;

    const getSortValue = (() => {
      switch (sortColumn?.t) {
        case "tree":
          return (idx: NodeIDX) => sortColumn.c.getNodeName(idx);
        case "numeric_value_column": {
          return (nodeIDX: NodeIDX) =>
            sortColumn.c.getNumericValues([nodeIDX])[0] as number;
        }
        case "non_sortable_column": {
          return null;
        }
        default: {
          const _exhaustiveCheck: never = sortColumn;
          throw new Error(`Unknown column type: ${_exhaustiveCheck}`);
        }
      }
    })();

    if (getSortValue == null) {
      return;
    }

    this.sortState = {
      sortColumn,
      sortOrder,
      sortFn: (a: Row, b: Row) => {
        const aValue = getSortValue(a.twinArrow.points_to);
        const bValue = getSortValue(b.twinArrow.points_to);
        if (aValue < bValue) {
          return sortOrder === "Desc" ? 1 : -1;
        }
        if (aValue > bValue) {
          return sortOrder === "Desc" ? -1 : 1;
        }
        return 0;
      },
    };
  }

  async resetTableAsync(setSortingProgress: SetSortingProgressFn) {
    this.rows = this.treeTableGraph.roots.map((nodeIDX) => {
      // Roots arrows are not "real" arrows, because arrows represent
      // edges and roots don't have edges leading to them. We create
      // default empty arrows for them to make the code simpler.
      const rootArrow = {
        tag: undefined,
        branch: undefined,
        properties: undefined,
        points_from: ARROW_POINTS_FROM_NON_EXISTENT,
        points_to: nodeIDX,
        points_to_unreachable: false,
        excluded: false,
        message: undefined,
        skipped: 0,
      };

      return {
        depth: 0,
        expanded: false,
        isCycle: false,
        parentRowRef: null,
        childrenRefs: [],
        transitiveChildrenCount: 0,
        twinArrow: {
          points_to: rootArrow.points_to,
          points_from: rootArrow.points_from,
          node_diff: 0,
          l: rootArrow,
          r: rootArrow,
        },
      };
    });
    await this.resortRowsAsync(setSortingProgress);
    this.forceUpdate();
  }

  async expandRowAsync(rowIDX: number, setProgress: SetSortingProgressFn) {
    const row = this.rows[rowIDX];
    if (row == null || row.expanded === true) {
      return;
    }

    const arrows = this.treeTableGraph.getTwinArrows(row.twinArrow.points_to);

    const childrenIDXs = arrows.map((a) => a.points_to);

    // if we're exanding the row we know that we'll need
    // that row to produce metrics/numberic values.
    // We can warm up the cache here by doing a batch call
    // that will populate caches for these rows all at once
    // instead of them going and fetching values one by one.
    // WASM<->JS calls are expensive and we want to minimize
    // them as much as possible.
    for (const column of this.columns) {
      if (column.t === "numeric_value_column") {
        if (this.sortState?.sortColumn === column) {
          // If we're sorting by this column we need to
          // get all the values for the children. We will need them
          // anyway to order the rows, even if they're virtualized and
          // not visible
          await this.warmUpNumericValuesCache(
            column,
            childrenIDXs,
            setProgress,
          );
        } else {
          // If we're not sorting by this column we can
          // just get a few. For most cases this will cover
          // whatever is displayed on the screen.
          column.c.getNumericValues(childrenIDXs.slice(0, 100));
        }
      }
    }

    expandRow(this.rows, rowIDX, arrows, this.sortState);
    this.forceUpdate();
  }

  collapseRow(rowIDX: number) {
    const row = this.rows[rowIDX];
    if (row == null || row.expanded === false) {
      return;
    }

    collapseRow(this.rows, rowIDX);
    this.forceUpdate();
  }

  async resortRowsAsync(setProgress: SetSortingProgressFn) {
    if (this.sortState == null) {
      return;
    }
    const selectedRow =
      this.selectedRowIDX != null ? this.rows[this.selectedRowIDX] : null;

    const allRowIDXsChuncked = this.rows.map((row) => row.twinArrow.points_to);
    const column = this.sortState.sortColumn;
    await this.warmUpNumericValuesCache(
      column,
      allRowIDXsChuncked,
      setProgress,
    );

    this.rows = sortRows(this.rows, this.sortState.sortFn);

    if (selectedRow != null) {
      // If we have a selected row we need to find it in the new
      // order and set the selectedRowIDX to it.
      const newSelectedRowIDX = this.rows.indexOf(selectedRow);
      if (newSelectedRowIDX !== -1) {
        // We reordered the rows and the selected row should remain
        // the same. It should be safe to directly update the selectedRowIDX
        this.selectedRowIDX = newSelectedRowIDX;
        this.scrollToSelected();
      }
    }
    this.forceUpdate();
  }

  // This is a helper function to warm up the numeric values cache
  // for a specific column and a set of nodeIDXs.
  // It will call the getNumericValues method on the column
  // to populate the cache.
  async warmUpNumericValuesCache(
    columnInternal: ColumnInternal | null,
    nodeIDXs: NodeIDX[],
    setProgress: SetSortingProgressFn,
  ) {
    const startTime = Date.now();
    let elapsed = 0;

    if (columnInternal == null || columnInternal.t !== "numeric_value_column") {
      return;
    }

    // Randomize the order of the nodeIDXs to avoid
    // having unevenly distributed value. Eg. if it was already sorted
    // by a similar column name, the heaviest rows to compute might be
    // at the top while the bottom will be instantaneous.
    // Randomizing will make the progress look more even
    const randomizedNodeIDXs = shuffleArray(nodeIDXs);

    const column = columnInternal.c;

    const computeChunk = async (
      c: NumericValueColumnDefinition,
      chunk: NodeIDX[],
    ) => {
      return new Promise<void>((resolve) => {
        // we do it in chunks to let the event loop breathe.
        // Otherwise we'll lock everything in a single synchronous
        // loop.
        setTimeout(() => {
          c.getNumericValues(chunk);
          resolve();
        }, 1);
      });
    };

    const total = randomizedNodeIDXs.length;
    let done = 0;

    const MIN_CHUNK_SIZE = 20;
    const MAX_CHUNK_SIZE = 10000;
    const TARGET_CHUNK_DURATION_MS = 100;
    const CHUNK_MULTIPLIER = 2;
    // We don't want to report progress for small warm ups.
    // to avoid flickering. We'll wait for a bit before actually
    // showing the progress bar.
    const REPORT_PROGRESS_AFTER_MS = 700;

    let currentChunkSize = MIN_CHUNK_SIZE;

    while (randomizedNodeIDXs.length > 0) {
      const chunkStartTime = Date.now();
      // pop `currentChunkSize` items from the array
      const chunk = randomizedNodeIDXs.splice(0, currentChunkSize);
      await computeChunk(column, chunk);
      done += chunk.length;
      const chunkTime = Date.now() - chunkStartTime;

      // Adjust chunk size for next iteration
      if (chunkTime < TARGET_CHUNK_DURATION_MS) {
        currentChunkSize = Math.min(
          currentChunkSize * CHUNK_MULTIPLIER,
          MAX_CHUNK_SIZE,
        );
      } else {
        currentChunkSize = Math.max(
          currentChunkSize / CHUNK_MULTIPLIER,
          MIN_CHUNK_SIZE,
        );
      }
      elapsed = Date.now() - startTime;
      if (elapsed > REPORT_PROGRESS_AFTER_MS) {
        // If we already spent some time on this, we can report progress
        setProgress([done, total]);
      }
    }

    // If we finished the warm up, we can set progress to null
    setProgress(null);
  }

  setSelectedRowIDX(rowIDX: number | null) {
    this.selectedRowIDX = rowIDX;
    const newSelectedPath: NodeIDX[] = [];
    let current: Row | null =
      rowIDX != null ? (this.rows[rowIDX] ?? null) : null;

    while (current != null) {
      newSelectedPath.push(current.twinArrow.points_to);
      current = current.parentRowRef;
    }
    newSelectedPath.reverse();
    this.selectedNodeIDXPath = newSelectedPath;
    this.forceUpdate();
  }

  scrollToSelected() {
    if (this.selectedRowIDX != null) {
      this.scrollToIndex(this.selectedRowIDX + 1, { align: "center" });
    }
  }

  clearSelectedRow() {
    this.setSelectedRowIDX(null);
  }

  navigateTop() {
    this.setSelectedRowIDX(0);
    this.forceUpdate();
    this.scrollToSelected();
  }

  navigateBottom() {
    this.setSelectedRowIDX(this.rows.length - 1);
    this.forceUpdate();
    this.scrollToSelected();
  }

  navigateUp(count: number) {
    const selectedRowIDX = this.selectedRowIDX ?? 0;
    if (this.selectedRowIDX === 0) {
      return; // we're already at the top
    }

    if (selectedRowIDX > 0) {
      this.setSelectedRowIDX(Math.max(selectedRowIDX - count, 0));
      this.forceUpdate();
      this.scrollToSelected();
    }
  }

  navigateDown(count: number) {
    if (this.selectedRowIDX === this.rows.length - 1) {
      return; // we're already at the bottom
    }
    const selectedRowIDX = this.selectedRowIDX ?? -1;
    this.setSelectedRowIDX(
      Math.max(0, Math.min(selectedRowIDX + count, this.rows.length - 1)),
    );
    this.forceUpdate();
    this.scrollToSelected();
  }

  navigateLeft() {
    const selectedRowIDX = this.selectedRowIDX ?? 0;
    const selectedRow = this.rows[selectedRowIDX];

    if (selectedRow) {
      if (selectedRow.expanded === true) {
        this.collapseRow(selectedRowIDX);
      } else {
        this.navigateUp(1);
      }
    }
  }
  async navigateRightAsync(setProgress: SetSortingProgressFn) {
    if (this.selectedRowIDX == null) {
      this.navigateDown(1);
      return;
    }
    const selectedRowIDX = this.selectedRowIDX ?? 0;
    const selectedRow = this.rows[selectedRowIDX];
    if (selectedRow) {
      if (
        selectedRow.expanded === true ||
        // TODO: getting all children to check for existence is
        // pretty heavy and we can optimize this to a simple
        // direct memory access check on a cached datastructure
        // or something.
        this.treeTableGraph.getTwinArrows(selectedRow.twinArrow.points_to)
          .length === 0 ||
        selectedRow.isCycle
      ) {
        this.navigateDown(1);
      } else {
        await this.expandRowAsync(selectedRowIDX, setProgress);
      }
    }
  }

  navigateToPath(path: NodeIDX[]) {
    const selectedRowIDX = expandToPath(
      this.rows,
      path,
      this.treeTableGraph.getTwinArrows,
      this.sortState,
    );

    if (selectedRowIDX != null) {
      this.setSelectedRowIDX(selectedRowIDX);
      this.forceUpdate();

      setTimeout(() => {
        this.scrollToSelected();
      }, 5);
    }
  }

  /// When we reset our table but want to keep the selected path we would call
  /// this.
  ///   -The path might still be valid (e.g. we switched from dominator tree to
  ///     refular graph and we had the row selected that didn't change the path)
  ///   -The path might not be valid anymore (e.g. we switched from a flat list
  ///    to a tree and the the node we had selected is not a root anymore)
  ///
  /// If the path if valid we would just select it. If the path is not valid, we'll
  /// try our best to find the shortest path to the node that was selected
  /// and navigate to that path.
  navigateToCurrentPathOrFallbackToShortestPath() {
    const currentPath = this.selectedNodeIDXPath;
    const treeTableEntryPoints = this.treeTableGraph.treeTableEntryPoints;
    if (currentPath.length === 0) {
      return; // no path to navigate to
    }
    const firstNodeIDX = currentPath[0] as NodeIDX;
    const selectedNodeIDX = currentPath[currentPath.length - 1] as NodeIDX;

    const isPathValid = () => {
      /// if the first node in the path is not a root then the path is invalid right away
      if (!this.treeTableGraph.roots.includes(firstNodeIDX)) {
        return false;
      }

      // If it's in the roots then we can try to follow every node in the path
      // and make sure that the NEXT node in the path is a child of the current node.
      for (let i = 0; i < currentPath.length - 1; i++) {
        const currentNodeIDX = currentPath[i] as NodeIDX;
        const nextNodeIDX = currentPath[i + 1] as NodeIDX;
        const arrows = this.treeTableGraph.getTwinArrows(currentNodeIDX);

        if (arrows.find((a) => a.points_to === nextNodeIDX) == null) {
          // our next node is not a child of the current node, which means the path is invalid
          return false;
        }
      }

      // if all the nodes in the path are valid and we successfully got to here our path is valid
      return true;
    };

    // If our entry points are a flat list ("AllReachable", or "Filtered" for a
    // narrowed-down flat list),
    // we normally don't want to navigate to a specific path. Most of the time you switch
    // into the flat list to see sorted values and then back to the tree table.
    // Expanding rows in a flat list make the sorting look weird and is generally not useful.
    // So for this case we will opt out of navigating to the set specific path and instead
    // skipp to finding the shortest path, which will be the path of the node itself, since
    // it's a flat list.
    const shouldUseValidPath = !isFlatListEntryPoints(treeTableEntryPoints);

    if (shouldUseValidPath && isPathValid()) {
      // If the path is valid we can just navigate to it
      this.navigateToPath(currentPath);
      return;
    }

    // if the path is invalid we can try to find the shortest path
    // to the last node in the path.
    const shortestPath = this.treeTableGraph.getShortestPath(
      this.treeTableGraph.roots,
      selectedNodeIDX,
    );

    if (shortestPath != null && shortestPath.length > 0) {
      // If we found a shortest path we can navigate to it
      this.navigateToPath(shortestPath);
    }
  }
}

type SetSortingProgressFn = (
  progress: [done: number, total: number] | null,
) => void;

export type SortState = {
  sortColumn: ColumnInternal | null;
  sortOrder: SortOrder | null;
  sortFn: SortFn;
};

function shuffleArray<T>(array: T[]): T[] {
  const shuffled = array.slice();
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [shuffled[i], shuffled[j]] = [shuffled[j] as T, shuffled[i] as T];
  }
  return shuffled;
}

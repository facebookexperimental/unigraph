// Copyright (c) Meta Platforms, Inc. and affiliates.

import { type VirtualItem, useVirtualizer } from "@tanstack/react-virtual";
import clsx from "clsx";
import {
  ArrowDown01,
  ArrowDownAZ,
  ArrowDownUp,
  ArrowLeftRight,
  ArrowUp10,
  ArrowUpZA,
  Dot,
  TreePalm,
} from "lucide-react";
import {
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useTransition,
} from "react";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../ArrowUtils";
import type { ArrowPair } from "../TwinGraph";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "../__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { GraphStructure } from "../__generated__/ts/GraphStructure";
import type { GraphTableSort } from "../__generated__/ts/GraphTableSort";
import type { SortOrder } from "../__generated__/ts/SortOrder";
import { Progress } from "../components/ui/progress";
import { useSelectedPath } from "../context/SelectedPathContext";
import type { NodeIDX } from "../types";
import { TreeCellDelta, TreeCellSingle } from "./TreeCell";
import {
  type Row,
  type SortFn,
  collapseRow,
  expandRow,
  expandToPath,
  sortRows,
} from "./TreeTableRows";

export type TreeColumnID = "__tree_column__";
export type ColumnID = string;

/// There must always be a "tree column" that renders node
/// names and is used for search/highlighting of the nodes.
export type ColumnDefinitions = {
  treeColumn: TreeColumnDefinition;
  /// The rest are regular columns, keyed by their column
  /// name to enforce uniqueness. Column names will be used
  /// to capture some information, like current sorting.
  columns: { [columnID: ColumnID]: NonTreeColumnDefinition };
};

export type CommonNonTreeColumnDefinitionFields = {
  flexGrow?: number;
  label: string;
  renderer: (row: Readonly<Row>) => React.ReactNode;
  isHidden: boolean;
  isLabelHidden?: boolean;
};

export type NumericValueColumnDefinition =
  CommonNonTreeColumnDefinitionFields & {
    t: "numeric_value_column";
    getNumericValues: (idx: NodeIDX[]) => Float32Array;
    sortable: boolean;
  };

export type NonSortableColumnDefinition =
  CommonNonTreeColumnDefinitionFields & {
    t: "non_sortable_column";
  };

export type NonTreeColumnDefinition =
  | NumericValueColumnDefinition
  | NonSortableColumnDefinition;

export type TreeColumnDefinition = {
  label: string;
  getNodeName: (idx: NodeIDX) => string;
  flexGrow?: number;
  isLabelHidden?: boolean;
};

type ColumnInternal =
  | {
      t: "tree";
      columnID: TreeColumnID;
      c: TreeColumnDefinition;
      isHidden: false; // tree column is kinda important and should not be hidden
    }
  | {
      t: "numeric_value_column";
      columnID: string;
      c: NumericValueColumnDefinition;
      isHidden: boolean;
    }
  | {
      t: "non_sortable_column";
      columnID: string;
      c: NonSortableColumnDefinition;
      isHidden: boolean;
    };

type SetSortingProgressFn = (
  progress: [done: number, total: number] | null,
) => void;

export function TreeTable(props: {
  columnDefinitions: ColumnDefinitions;
  treeTableGraph: TreeTableGraph;
  headerHeight?: number;
  focusOnMount?: boolean;
  sortColumnID: TreeColumnID | string | null;
  sortOrder: SortOrder | null;
  onSortChange: (sort: GraphTableSort | null) => void;
}) {
  const [sortingProgress, setSortingProgress] = useState<
    null | [number, number]
  >(null);
  const [isPending, startTransition] = useTransition();

  const forceUpdate = useReducer((x) => {
    return x + 1;
  }, 0)[1];

  const { selectedPath, setSelectedPath, pathSelector, setSelectedRow } =
    useSelectedPath();

  const columns = useMakeInternalColumns(props.columnDefinitions);
  // Initial setup of stateful context. This runs only once
  // when the component is mounted and is never updated again.
  // We mutate it directly and manage the state of the table
  // manually.
  //
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  const ctx = useMemo(() => {
    const ctx = new TreeTableCtx(
      columns,
      selectedPath ?? [],
      // We start from an empty graph. This will render an empty table
      // and once we get the first render done the effects will kick in
      // and we will update the graph to the actual graph that is passed
      // with props and perform all needed initialization/sorting/etc.
      // This is done because the initial render might be extremely heavy.
      // Eg. if we have a massive graph that we want to sort by some
      // transitive column (which can take multiple minutes).
      // So we want to render/mount the table and then start rendering
      // loading progress bars and stuff instead of just being synchronously
      // stuck in the initial render
      EMPTY_TREE_TABLE_GRAPH,
    );
    ctx.updateSortState(props.sortColumnID, props.sortOrder);
    ctx.forceUpdate = forceUpdate;

    return ctx;
  }, []);

  const parentRef = useRef<HTMLDivElement>(null); // scrollable element for virtualizer

  const headerHeight = props.headerHeight ?? 35;

  useEffect(() => {
    pathSelector.navigate = (path: NodeIDX[] | null) => {
      ctx.navigateToPath(path ?? []);
    };
    if (ctx.selectedNodeIDXPath.length > 0) {
      ctx.navigateToPath(ctx.selectedNodeIDXPath);
    }
  }, [pathSelector, ctx]);

  useEffect(() => {
    ctx.columns = columns;
    ctx.updateSortState(props.sortColumnID, props.sortOrder);

    // if the graph changed we nuke the whole table
    // and start over from clean state. This could have been triggerred
    // by switching to a different graph mode (e.g. reverse/dominator) or
    // completely changing the graph, so we need to start over.
    if (props.treeTableGraph !== ctx.treeTableGraph) {
      startTransition(async () => {
        ctx.treeTableGraph = props.treeTableGraph;
        await ctx.resetTableAsync(setSortingProgress);
        ctx.navigateToCurrentPathOrFallbackToShortestPath();
      });
    } else {
      // otherwise if the sorting changed we'd want to resort the rows
      // and keep the graph the same.
      startTransition(async () => {
        await ctx.resortRowsAsync(setSortingProgress);
      });
    }
  }, [props.sortColumnID, props.sortOrder, ctx, columns, props.treeTableGraph]);

  useEffect(() => {
    setSelectedPath(ctx.selectedNodeIDXPath);
  }, [ctx.selectedNodeIDXPath, setSelectedPath]);

  const selectedRow =
    ctx.selectedRowIDX !== null ? (ctx.rows[ctx.selectedRowIDX] ?? null) : null;

  useEffect(() => {
    setSelectedRow(selectedRow);
  }, [selectedRow, setSelectedRow]);

  useEffect(() => {
    if (props.focusOnMount === true) {
      setTimeout(() => {
        if (parentRef.current != null) {
          parentRef.current.focus();
        }
      }, 0);
    }
  }, [props.focusOnMount]);

  const ITEM_SIZE = 35;

  const TreeCellComponent = props.treeTableGraph.isDeltaGraph
    ? TreeCellDelta
    : TreeCellSingle;

  const rowVirtualizer = useVirtualizer({
    count: ctx.rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ITEM_SIZE,
  });
  ctx.scrollToIndex = rowVirtualizer.scrollToIndex;

  const virtualItems = rowVirtualizer.getVirtualItems();
  const virtualIndexes = rowVirtualizer.getVirtualIndexes();

  // If we're too far in and all the depths are huge we don't wanna render
  // all of the padding dots and we can trim them to the minimum
  // instead of:
  // * * * A
  // * * * * B
  // * * * * * C
  //
  // we can do:
  // * A
  // * * B
  // * * * C
  let minDepth = ctx.rows[virtualItems[0]?.index ?? 0]?.depth ?? 0;
  for (let i = 0; i < virtualItems.length; i++) {
    const item = virtualItems[i] as VirtualItem;
    const row = ctx.rows[item.index];
    if (row == null) {
      continue;
    }
    if (row.depth < minDepth) {
      minDepth = row.depth;
    }
  }

  const paddingComponent = useMemo(() => {
    switch (props.treeTableGraph.graphStructure) {
      case "Forward": {
        return Dot;
      }
      case "Dominator": {
        return TreePalm;
      }
      case "Reverse": {
        return ArrowLeftRight;
      }
    }
  }, [props.treeTableGraph.graphStructure]);

  const columnsElements = columns.map((column, columnIDX) => {
    if (column.isHidden === true) {
      return null; // skip hidden columns
    }

    const columnID = column.columnID;
    const sortOrder = columnID === props.sortColumnID ? props.sortOrder : null;

    const columnCells = virtualItems.map((virtualItem) => {
      const rowIDX = virtualItem.index;
      const row = ctx.rows[rowIDX];
      if (row == null) {
        return null;
      }
      const selected = ctx.selectedRowIDX === rowIDX;
      const cell = (() => {
        const t = column.t;
        switch (t) {
          case "tree": {
            return (
              <TreeCellComponent
                row={row}
                minDepth={minDepth}
                paddingComponent={paddingComponent}
                onToggleExpand={(expanded) => {
                  if (expanded) {
                    startTransition(async () => {
                      await ctx.expandRowAsync(rowIDX, setSortingProgress);
                    });
                  } else {
                    ctx.collapseRow(rowIDX);
                  }
                }}
                canExpand={
                  props.treeTableGraph.getArrowPairs(row.arrow_pair.points_to)
                    .length > 0
                }
                nodeName={column.c.getNodeName(row.arrow_pair.points_to)}
              />
            );
          }
          case "numeric_value_column": {
            return column.c.renderer(row);
          }
          case "non_sortable_column": {
            return column.c.renderer(row);
          }
          default: {
            const _exhaustiveCheck: never = column;
            throw new Error(`Unknown column type: ${_exhaustiveCheck}`);
          }
        }
      })();
      return (
        <div
          className={clsx(
            "border-b box-border-gray-200 flex items-center cursor-pointer",
            selected &&
              "box-border border-t border-b border-primary/50 bg-primary/4",
          )}
          onMouseDown={(_e) => {
            ctx.setSelectedRowIDX(rowIDX);
          }}
          key={virtualItem.key}
          style={{
            height: `${virtualItem.size}px`,
          }}
        >
          {cell}
        </div>
      );
    });

    const isSortable =
      (column.t === "numeric_value_column" &&
        column.c.getNumericValues != null) ||
      column.t === "tree";

    const sortIcon = (() => {
      const isNumeric = column.t === "numeric_value_column";
      if (isSortable === false) {
        return null;
      }
      if (sortOrder === "Asc") {
        const Icon = isNumeric ? ArrowDown01 : ArrowDownAZ;

        return (
          <Icon
            size={16}
            className="mx-2 cursor-pointer"
            onClick={() =>
              props.onSortChange({ column_id: columnID, order: "Desc" })
            }
          />
        );
      } else if (sortOrder === "Desc") {
        const Icon = isNumeric ? ArrowUp10 : ArrowUpZA;
        return (
          <Icon
            size={16}
            className="mx-2 cursor-pointer"
            onClick={() =>
              props.onSortChange({ column_id: columnID, order: "Asc" })
            }
          />
        );
      } else {
        return (
          <ArrowDownUp
            size={16}
            className="mx-2 cursor-pointer"
            onClick={() =>
              props.onSortChange({ column_id: columnID, order: "Desc" })
            }
          />
        );
      }
    })();

    return (
      <div
        // biome-ignore lint/suspicious/noArrayIndexKey: <explanation>
        key={columnIDX}
        style={{
          height: `${rowVirtualizer.getTotalSize() + headerHeight}px`,
          flexGrow: column.c.flexGrow ?? 0,
        }}
      >
        <div
          style={{ height: `${headerHeight}px` }}
          className={clsx(
            "sticky top-0 border-b px-4 flex items-center whitespace-nowrap",
            sortOrder != null ? "bg-primary" : "bg-accent",
          )}
        >
          {column.c.isLabelHidden === true ? "" : column.c.label}
          {sortIcon}
        </div>
        <div
          style={{
            height: `${(virtualIndexes[0] ?? 0) * ITEM_SIZE}px`,
          }}
        />
        {columnCells}
      </div>
    );
  });

  return (
    <div className="relative flex flex-col grow shrink min-h-0">
      {/* The scrollable element  */}
      <div
        ref={parentRef}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: <explanation>
        tabIndex={0}
        className="overflow-auto h-full w-full outline-0"
        onKeyDown={(e) => {
          switch (e.key) {
            case "ArrowDown": {
              e.preventDefault();
              ctx.navigateDown(1);
              break;
            }
            case "ArrowUp": {
              e.preventDefault();
              ctx.navigateUp(1);
              break;
            }
            case "PageDown": {
              e.preventDefault();
              ctx.navigateDown(10);
              break;
            }
            case "PageUp": {
              e.preventDefault();
              ctx.navigateUp(10);
              break;
            }
            case "ArrowRight": {
              e.preventDefault();
              startTransition(async () => {
                await ctx.navigateRightAsync(setSortingProgress);
              });
              break;
            }
            case "ArrowLeft": {
              e.preventDefault();
              ctx.navigateLeft();
              break;
            }
          }
        }}
      >
        <div
          className="flex w-full relative"
          style={{
            height: `${rowVirtualizer.getTotalSize()}px`,
          }}
        >
          {columnsElements}
        </div>
      </div>
      {isPending && <SortingProgress progress={sortingProgress} />}
    </div>
  );
}

export type SortState = {
  sortFn: SortFn;
  sortColumnID: TreeColumnID | ColumnID;
  sortColumn: ColumnInternal | null;
};

class TreeTableCtx {
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

  updateSortState(
    columnID: TreeColumnID | ColumnID | null,
    order: SortOrder | null,
  ) {
    if (columnID == null || order == null) {
      this.sortState = null;
      return;
    }

    const column = this.columns.find((c) => c.columnID === columnID);
    if (column == null) {
      this.sortState = null;
      return;
    }

    const getSortValue = (() => {
      switch (column?.t) {
        case "tree":
          return (idx: NodeIDX) => column.c.getNodeName(idx);
        case "numeric_value_column": {
          return (nodeIDX: NodeIDX) =>
            column.c.getNumericValues([nodeIDX])[0] as number;
        }
        case "non_sortable_column": {
          return null;
        }
        default: {
          const _exhaustiveCheck: never = column;
          throw new Error(`Unknown column type: ${_exhaustiveCheck}`);
        }
      }
    })();

    if (getSortValue == null) {
      this.sortState = null;
      return;
    }

    this.sortState = {
      sortColumnID: columnID,
      sortColumn: column ?? null,
      sortFn: (a: Row, b: Row) => {
        const aValue = getSortValue(a.arrow_pair.points_to);
        const bValue = getSortValue(b.arrow_pair.points_to);
        if (aValue < bValue) {
          return order === "Desc" ? 1 : -1;
        }
        if (aValue > bValue) {
          return order === "Desc" ? -1 : 1;
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
      };

      return {
        depth: 0,
        expanded: false,
        isCycle: false,
        parentRowRef: null,
        childrenRefs: [],
        transitiveChildrenCount: 0,
        arrow_pair: {
          points_to: rootArrow.points_to,
          points_from: rootArrow.points_from,
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

    const arrows = this.treeTableGraph.getArrowPairs(row.arrow_pair.points_to);

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
        if (this.sortState?.sortColumnID === column.columnID) {
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

    const allRowIDXsChuncked = this.rows.map((row) => row.arrow_pair.points_to);
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
      const newSelectedRowIDX = this.rows.findIndex(
        // This should be the same object and we compare
        // by reference.
        (row) => row === selectedRow,
      );
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

  setSelectedRowIDX(rowIDX: number) {
    this.selectedRowIDX = rowIDX;
    const newSelectedPath: NodeIDX[] = [];
    let current: Row | null = this.rows[rowIDX] ?? null;
    while (current != null) {
      newSelectedPath.push(current.arrow_pair.points_to);
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
        this.treeTableGraph.getArrowPairs(selectedRow.arrow_pair.points_to)
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
      this.treeTableGraph.getArrowPairs,
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
        const arrows = this.treeTableGraph.getArrowPairs(currentNodeIDX);

        if (arrows.find((a) => a.points_to === nextNodeIDX) == null) {
          // our next node is not a child of the current node, which means the path is invalid
          return false;
        }
      }

      // if all the nodes in the path are valid and we successfully got to here our path is valid
      return true;
    };

    // If our entry points are set to "AllReachable", which is essentially a flat list,
    // we normally don't want to navigate to a specific path. Most of the time you switch
    // into the flat list to see sorted values and then back to the tree table.
    // Expanding rows in a flat list make the sorting look weird and is generally not useful.
    // So for this case we will opt out of navigating to the set specific path and instead
    // skipp to finding the shortest path, which will be the path of the node itself, since
    // it's a flat list.
    const shouldUseValidPath = treeTableEntryPoints !== "AllReachable";

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

function useMakeInternalColumns(
  columnDefinitions: ColumnDefinitions,
): Array<ColumnInternal> {
  return useMemo(() => {
    const result: Array<ColumnInternal> = [
      {
        t: "tree",
        columnID: "__tree_column__",
        isHidden: false,
        c: columnDefinitions.treeColumn,
      },
    ];

    for (const [columnID, column] of Object.entries(
      columnDefinitions.columns,
    )) {
      switch (column.t) {
        case "numeric_value_column":
          {
            result.push({
              t: "numeric_value_column",
              columnID,
              isHidden: column.isHidden,
              c: column,
            });
          }
          break;
        case "non_sortable_column": {
          result.push({
            t: "non_sortable_column",
            columnID,
            isHidden: column.isHidden,
            c: column,
          });
          break;
        }
        default: {
          const _exhaustiveCheck: never = column;
          throw new Error(`Unknown column type: ${_exhaustiveCheck}`);
        }
      }
    }

    return result;
  }, [columnDefinitions]);
}

// Some super sketchy stuff to make it possible to
// select a new path from the outside of the component.
// There's probably a much better way to do this, but i have
// no idea how so yolo.
// This class will be created in the outside world and passed
// down to the TreeTable component.
// If someone calls setNewSelectedPath on this class, it will
// set the selected path on the TreeTable component and make it
// do all the expanding/scrolling to the new selected path/node.
export class TreeTablePathSelector {
  setNewSelectedPath: (path: NodeIDX[]) => void;
  initialPath: NodeIDX[] | null;

  constructor(initialPath: NodeIDX[] | null) {
    this.initialPath = initialPath;
    this.setNewSelectedPath = () => {};
  }
}

/// Object that defines the structure of the graph represented
/// in the tree table and how to havigate it
type TreeTableGraph = {
  roots: Readonly<NodeIDX[]>;
  getArrowPairs: (idx: NodeIDX) => ArrowPair[];
  getShortestPath: (from: readonly NodeIDX[], to: NodeIDX) => NodeIDX[] | null;
  graphStructure: GraphStructure;
  treeTableEntryPoints: ArrayGraphUISettingsTreeTableEntryPoints;
  // whether we're rendering a single graph or comparing two graphs
  isDeltaGraph: boolean;
};

const EMPTY_TREE_TABLE_GRAPH: TreeTableGraph = {
  roots: [],
  getArrowPairs: () => [],
  getShortestPath: () => null,
  graphStructure: "Forward",
  treeTableEntryPoints: "Determine",
  isDeltaGraph: false,
};

function SortingProgress({
  progress,
}: {
  progress: null | [done: number, total: number];
}) {
  if (progress == null || progress[1] < 1000) {
    return null; // no progress to show
  }
  return (
    <div className="absolute top-0 left-0 right-0 bottom-0 w-full h-full bg-[rgba(0,0,0,0.8)] flex items-center justify-center">
      <div className="flex flex-col items-center w-[600px] bg-card p-4 px-8 rounded-lg">
        <p className="text-lg text-foreground mb-4">
          Sorting rows{" "}
          {progress != null ? `${progress[0]} / ${progress[1]}` : "..."}
        </p>
        <p className="text-sm text-muted-foreground mb-4">
          Sorting requires computing numeric values for each row, which is a
          quadratic operation for transitive columns and might take a while.
        </p>
        {progress != null ? (
          <Progress
            value={(100 * progress[0]) / progress[1]}
            className="w-full mb-4"
          />
        ) : (
          "Loading..."
        )}
      </div>
    </div>
  );
}

function shuffleArray<T>(array: T[]): T[] {
  const shuffled = array.slice();
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [shuffled[i], shuffled[j]] = [shuffled[j] as T, shuffled[i] as T];
  }
  return shuffled;
}

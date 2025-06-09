// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
import type { NodeIDX } from "../types";
import { useEffect, useMemo, useReducer, useRef } from "react";
import clsx from "clsx";
import {
  ArrowDown01,
  ArrowDownAZ,
  ArrowDownUp,
  ArrowUp10,
  ArrowUpZA,
} from "lucide-react";
import {
  collapseRow,
  expandRow,
  expandToPath,
  type Row,
  type SortFn,
  sortRows,
} from "./TreeTableRows";
import type { Arrow } from "../../u-be/unigraph_core/bindings/Arrow";
import TreeCell from "./TreeCell";
import type { SortOrder } from "u-be/unigraph_core/bindings/SortOrder";
import type { GraphTableSort } from "u-be/unigraph_core/bindings/GraphTableSort";

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
  renderer: (arrow: Arrow) => React.ReactNode;
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

export function TreeTable(props: {
  columnDefinitions: ColumnDefinitions;
  roots: NodeIDX[];
  headerHeight?: number;
  focusOnMount?: boolean;
  onSelectedNodeIDXPathChange: (path: NodeIDX[]) => void;
  pathSelector: TreeTablePathSelector;
  sortColumnID: TreeColumnID | string | null;
  sortOrder: SortOrder | null;
  onSortChange: (sort: GraphTableSort | null) => void;
  getArrows: (idx: NodeIDX) => Arrow[];
}) {
  const forceUpdate = useReducer((x) => {
    return x + 1;
  }, 0)[1];

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
      props.roots,
      props.pathSelector.initialPath ?? [],
    );
    ctx.getArrows = props.getArrows;
    ctx.updateSortState(props.sortColumnID, props.sortOrder);
    ctx.forceUpdate = forceUpdate;
    return ctx;
  }, []);

  const parentRef = useRef<HTMLDivElement>(null); // scrollable element for virtualizer

  const headerHeight = props.headerHeight ?? 35;

  useEffect(() => ctx.updateRoots(props.roots), [props.roots, ctx]);

  useEffect(() => {
    props.pathSelector.setNewSelectedPath = (path: NodeIDX[]) => {
      ctx.navigateToPath(path);
    };
    if (ctx.selectedNodeIDXPath.length > 0) {
      ctx.navigateToPath(ctx.selectedNodeIDXPath);
    }
  }, [props.pathSelector, ctx]);

  useEffect(() => {
    ctx.getArrows = props.getArrows;
    ctx.columns = columns;
    ctx.updateSortState(props.sortColumnID, props.sortOrder);
    ctx.resortRows();
  }, [props.sortColumnID, props.sortOrder, ctx, columns, props.getArrows]);

  useEffect(() => {
    props.onSelectedNodeIDXPathChange(ctx.selectedNodeIDXPath);
  }, [ctx.selectedNodeIDXPath, props.onSelectedNodeIDXPathChange]);

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
      const selected = ctx.selectedRowIDX === rowIDX && " bg-primary";
      const cell = (() => {
        const t = column.t;
        switch (t) {
          case "tree": {
            return (
              <TreeCell
                row={row}
                minDepth={minDepth}
                onToggleExpand={(expanded) => {
                  if (expanded) {
                    ctx.expandRow(rowIDX);
                  } else {
                    ctx.collapseRow(rowIDX);
                  }
                }}
                canExpand={props.getArrows(row.arrow.points_to).length > 0}
                nodeName={column.c.getNodeName(row.arrow.points_to)}
              />
            );
          }
          case "numeric_value_column": {
            return column.c.renderer(row.arrow);
          }
          case "non_sortable_column": {
            return column.c.renderer(row.arrow);
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
            selected,
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
              props.onSortChange({ column_id: columnID, order: "Asc" })
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
            "sticky top-0 border-b  px-4 flex items-center",
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
    <>
      {/* The scrollable element for your list */}
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
              ctx.navigateRight();
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
    </>
  );
}

export type SortState = {
  sortFn: SortFn;
  sortColumnID: TreeColumnID | ColumnID;
};

class TreeTableCtx {
  rows: Array<Row>;
  columns: ColumnInternal[];
  forceUpdate: () => void;
  sortState: SortState | null;
  getArrows: (idx: NodeIDX) => Arrow[] = () => [];
  selectedRowIDX: number | null;
  selectedNodeIDXPath: NodeIDX[];
  scrollToIndex: (index: number, options: { align: "center" }) => void;

  constructor(
    columns: ColumnInternal[],
    roots: NodeIDX[],
    selectedNodeIDXPath: NodeIDX[],
  ) {
    this.columns = columns;
    this.rows = [];
    this.forceUpdate = () => {};
    this.sortState = null;
    this.updateRoots(roots);
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
      sortFn: (a: Row, b: Row) => {
        const aValue = getSortValue(a.arrow.points_to);
        const bValue = getSortValue(b.arrow.points_to);
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

  updateRoots(roots: NodeIDX[]) {
    this.rows = roots.map((nodeIDX) => {
      return {
        depth: 0,
        expanded: false,
        isCycle: false,
        // Roots arrows are not "real" arrows, because arrows represent
        // edges and roots don't have edges leading to them. We create
        // default empty arrows for them to make the code simpler.
        arrow: {
          tag: null,
          branch: null,
          properties: null,
          // JS indexes are not u32 so -1 is a valid value.
          // We're going to abuse it to represent the root arrows
          points_from: -1,
          points_to: nodeIDX,
          points_to_unreachable: false,
          excluded: false,
        },
        parentRowRef: null,
        childrenRefs: [],
        transitiveChildrenCount: 0,
      };
    });
    this.resortRows();
    this.forceUpdate();
  }

  expandRow(rowIDX: number) {
    const row = this.rows[rowIDX];
    if (row == null || row.expanded === true) {
      return;
    }

    const arrows = this.getArrows(row.arrow.points_to);

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
          column.c.getNumericValues(childrenIDXs);
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

  resortRows() {
    if (this.sortState == null) {
      return;
    }
    const selectedRow =
      this.selectedRowIDX != null ? this.rows[this.selectedRowIDX] : null;

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

  setSelectedRowIDX(rowIDX: number) {
    this.selectedRowIDX = rowIDX;
    const newSelectedPath: NodeIDX[] = [];
    let current: Row | null = this.rows[rowIDX] ?? null;
    while (current != null) {
      newSelectedPath.push(current.arrow.points_to);
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
  navigateRight() {
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
        this.getArrows(selectedRow.arrow.points_to).length === 0 ||
        selectedRow.isCycle
      ) {
        this.navigateDown(1);
      } else {
        this.expandRow(selectedRowIDX);
      }
    }
  }

  navigateToPath(path: NodeIDX[]) {
    const selectedRowIDX = expandToPath(
      this.rows,
      path,
      this.getArrows,
      this.sortState,
    );

    if (selectedRowIDX != null) {
      this.setSelectedRowIDX(selectedRowIDX);
      this.forceUpdate();

      setTimeout(() => {
        this.scrollToSelected();
      }, 0);
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
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
import { useEffect, useMemo, useReducer, useState, useTransition } from "react";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "../__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { GraphStructure } from "../__generated__/ts/GraphStructure";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import UHoverCard from "../components/UHoverCard";
import { Progress } from "../components/ui/progress";
import { useTreeTableRef } from "../context/GlobalElementRefs";
import { useSelectedPath } from "../context/SelectedPathContext";
import type { NodeIDX } from "../types";
import { type ColumnDefinitions, useMakeInternalColumns } from "./columns";
import TreeCell from "./TreeCell";
import { TreeTableCtx } from "./TreeTableCtx";

export function TreeTable(props: {
  columnDefinitions: ColumnDefinitions;
  treeTableGraph: TreeTableGraph;
  headerHeight?: number;
  focusOnMount?: boolean;
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once
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
    ctx.updateSortState();
    ctx.forceUpdate = forceUpdate;

    return ctx;
  }, []);

  const parentRef = useTreeTableRef();

  const headerHeight = props.headerHeight ?? 35;

  useEffect(() => {
    pathSelector.navigate = (path: NodeIDX[] | null) => {
      ctx.selectedNodeIDXPath = path ?? [];
      ctx.navigateToCurrentPathOrFallbackToShortestPath();
    };
    if (ctx.selectedNodeIDXPath.length > 0) {
      ctx.navigateToPath(ctx.selectedNodeIDXPath);
    }
  }, [pathSelector, ctx]);

  useEffect(() => {
    ctx.columns = columns;
    ctx.updateSortState();

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
  }, [ctx, columns, props.treeTableGraph]);

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
  }, [props.focusOnMount, parentRef]);

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

    const SelectedSortOrder =
      ctx.sortState?.sortColumn === column ? ctx.sortState.sortOrder : null;

    const columnCells = virtualItems.map((virtualItem) => {
      const rowIDX = virtualItem.index;
      const row = ctx.rows[rowIDX];
      if (row == null) {
        return null;
      }
      const selected = ctx.selectedRowIDX === rowIDX;
      let testId: string | undefined;
      const cell = (() => {
        const t = column.t;
        switch (t) {
          case "tree": {
            const nodeName = column.c.getNodeName(row.twinArrow.points_to);
            testId = `node-row-${nodeName}`;
            return (
              <TreeCell
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
                  props.treeTableGraph.getTwinArrows(row.twinArrow.points_to)
                    .length > 0
                }
                nodeName={nodeName}
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
            selected && "bg-gray-200",
          )}
          data-testid={testId}
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

    const sortable =
      column.t !== "non_sortable_column" ? column.c.sortable : null;

    const sortIcon = (() => {
      const isNumeric = column.t === "numeric_value_column";
      if (sortable == null) {
        return null;
      }
      if (SelectedSortOrder === "Asc") {
        const Icon = isNumeric ? ArrowDown01 : ArrowDownAZ;

        return (
          <Icon
            size={16}
            className="mx-2 cursor-pointer"
            onClick={() => sortable.onSortChange("Desc")}
          />
        );
      } else if (SelectedSortOrder === "Desc") {
        const Icon = isNumeric ? ArrowUp10 : ArrowUpZA;
        return (
          <Icon
            size={16}
            className="mx-2 cursor-pointer"
            onClick={() => sortable.onSortChange("Asc")}
          />
        );
      } else {
        return (
          <ArrowDownUp
            size={16}
            className="mx-2 cursor-pointer"
            onClick={() => sortable.onSortChange("Desc")}
          />
        );
      }
    })();

    return (
      <div
        // biome-ignore lint/suspicious/noArrayIndexKey: because
        key={columnIDX}
        style={{
          height: `${rowVirtualizer.getTotalSize() + headerHeight}px`,
          flexGrow: column.c.flexGrow ?? 0,
          // Allow the flexible tree column to shrink below its content size so
          // very long node names truncate instead of widening the whole table.
          minWidth: column.t === "tree" ? 0 : undefined,
        }}
      >
        <div
          style={{ height: `${headerHeight}px` }}
          className={clsx(
            "sticky top-0 border-b px-4 flex w-full items-center whitespace-nowrap",
            SelectedSortOrder != null
              ? "bg-primary text-background"
              : "bg-accent",
          )}
        >
          {column.c.isLabelHidden === true ? (
            ""
          ) : (
            <UHoverCard asChild content={column.c.hovercardContent ?? null}>
              <span>{column.c.label}</span>
            </UHoverCard>
          )}
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
    <div className="relative flex flex-col grow shrink min-h-0 min-w-0">
      {/* The scrollable element  */}
      <div
        ref={parentRef}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: because
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
            case "Home": {
              e.preventDefault();
              ctx.navigateTop();
              break;
            }
            case "End": {
              e.preventDefault();
              ctx.navigateBottom();
              break;
            }
            case "Escape": {
              e.preventDefault();
              ctx.clearSelectedRow();
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
export type TreeTableGraph = {
  roots: Readonly<NodeIDX[]>;
  getTwinArrows: (idx: NodeIDX) => TwinArrow[];
  getShortestPath: (from: readonly NodeIDX[], to: NodeIDX) => NodeIDX[] | null;
  graphStructure: GraphStructure;
  treeTableEntryPoints: ArrayGraphUISettingsTreeTableEntryPoints;
};

const EMPTY_TREE_TABLE_GRAPH: TreeTableGraph = {
  roots: [],
  getTwinArrows: () => [],
  getShortestPath: () => null,
  graphStructure: "Forward",
  treeTableEntryPoints: "Determine",
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

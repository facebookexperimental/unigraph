// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useMemo } from "react";
import type { NodeIDX } from "../__generated__/ts/NodeIDX";
import type { SortOrder } from "../__generated__/ts/SortOrder";
import type { Row } from "./TreeTableRows";

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
  hovercardContent?: React.ReactNode;
};

export type TSortable = {
  // If present, that means the table is sorted by this column with
  // the provided sort order
  order: SortOrder | null;
  onSortChange: (sort: SortOrder | null) => void;
};

export type NumericValueColumnDefinition =
  CommonNonTreeColumnDefinitionFields & {
    t: "numeric_value_column";
    getNumericValues: (idx: NodeIDX[]) => Float32Array;
    sortable: TSortable | null;
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
  sortable: TSortable | null;
  hovercardContent?: React.ReactNode;
};

export type ColumnInternal =
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

export function useMakeInternalColumns(
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

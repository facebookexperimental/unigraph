// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { ColumnType } from "../../__generated__/ts/ColumnType";
import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import type { NodeIDX } from "../../__generated__/ts/NodeIDX";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../../ArrowUtils";
import UHoverCard from "../../components/UHoverCard";
import NativeGraph, {
  GRAPH_SIDE,
  type GraphSide,
} from "../../native/NativeGraph";
import type TwinGraph from "../../native/TwinGraph";
import { H1, H2, Link, Pre } from "../../Typography";
import type { NumericValueColumnDefinition, TSortable } from "../TreeTable";
import type { Row } from "../TreeTableRows";
import {
  DeltaMetricCell,
  MetricCell,
  MissingMetric,
  NO_PRECISION_FORMAT,
  WouldBeDeltaMetricCell,
} from "./Cells";
import type { Column, ColumnsCtx } from "./useGraphTreeTableColumns";

export class TransitiveCountColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  side: GraphSide | null;
  showWouldBe = false;

  constructor(ctx: ColumnsCtx, nativeGraph: NativeGraph, side?: GraphSide) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.side = side ?? null;
  }

  isEnabled() {
    return this.ctx.showTransitiveCount && this.ctx.showTransitive;
  }

  sortable(): TSortable | null {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TransitiveCount: {
            t: columnType,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TransitiveCount" in sort.column &&
      sort.column.TransitiveCount.t === columnType
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float32Array {
    if (this.ctx.dominated) {
      return (idxs: NodeIDX[]) =>
        this.nativeGraph.getTransitiveCountDominated(idxs);
    } else {
      return (idxs: NodeIDX[]) => this.nativeGraph.getTransitiveCount(idxs);
    }
  }

  getID(): string {
    const base = this.ctx.dominated ? "D(count)" : "T(count)";
    if (this.side == null) {
      return base;
    }
    return `${base} ${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const getValues = this.getValuesFn();
    const columnID = this.getID();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={getValues([row.twinArrow.points_to])[0] ?? 0}
              format={NO_PRECISION_FORMAT}
            />
          );
        } else if (
          row.twinArrow.points_from === ARROW_POINTS_FROM_NON_EXISTENT
        ) {
          // If the arrow is coming from a non-existent point, we don't know
          // what to override
          return <MissingMetric />;
        } else {
          const currentValue =
            this.nativeGraph.getCombinedMetricsForEntryPoints().node_count ?? 0;
          const wouldBeValue =
            this.nativeGraph.getCombinedMetricsForEntryPointsWithOverrides({
              from: row.twinArrow.points_from,
              to: row.twinArrow.points_to,
            }).node_count ?? 0;
          return this.showWouldBe ? (
            <WouldBeDeltaMetricCell
              value={wouldBeValue - currentValue}
              format={NO_PRECISION_FORMAT}
            />
          ) : (
            <MissingMetric />
          );
        }
      },
      getNumericValues: getValues,
      sortable: this.sortable(),
      isHidden: false,
      hovercardContent: this.ctx.dominated ? (
        <TransitiveDominatedCountHovercard />
      ) : (
        <TransitiveCountHovercard />
      ),
    };
    return [columnID, definition];
  }
}

export class TransitiveCountDeltaColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
  }

  isEnabled() {
    return this.ctx.showTransitiveCount && this.ctx.showTransitive;
  }

  sortable(): TSortable | null {
    const columnType: ColumnType = "Delta";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TransitiveCount: {
            t: columnType,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TransitiveCount" in sort.column &&
      sort.column.TransitiveCount.t === columnType
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float32Array {
    return (idxs: NodeIDX[]) => this.twinGraph.getTransitiveCountDelta(idxs);
  }

  /// when we sort delta column we want to sort by absolute value
  /// to see "what changed the most". Sorting it as is will push
  /// negative values all the way to the bottom below zeros.
  getValuesFnForSorting(): (idxs: NodeIDX[]) => Float32Array {
    return (idxs: NodeIDX[]) =>
      this.twinGraph.getTransitiveCountDelta(idxs).map(Math.abs);
  }

  getID(): string {
    return "∆T(count)";
  }

  definition(): [string, NumericValueColumnDefinition] {
    const getValues = this.getValuesFn();
    const getValuesForSorting = this.getValuesFnForSorting();
    const columnID = this.getID();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        return (
          <DeltaMetricCell
            value={getValues([row.twinArrow.points_to])[0] ?? 0}
            format={NO_PRECISION_FORMAT}
          />
        );
      },
      getNumericValues: getValuesForSorting,
      sortable: this.sortable(),
      isHidden: false,
      hovercardContent: this.ctx.dominated ? (
        <TransitiveDominatedCountHovercard />
      ) : (
        <TransitiveCountHovercard />
      ),
    };
    return [columnID, definition];
  }
}

export class TransitiveCountRightInDeltaViewColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
  }

  isEnabled() {
    return (
      this.ctx.showTransitiveCount &&
      this.ctx.showTransitive &&
      this.twinGraph.r != null
    );
  }

  sortable(): TSortable | null {
    const columnType: ColumnType = "Right";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TransitiveCount: {
            t: columnType,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TransitiveCount" in sort.column &&
      // This column sorta represents both left and right graphs while
      // only showing the right value. Since left column does not exist
      // we will take the `Left` sorting as sorting for `Right` as well.
      (sort.column.TransitiveCount.t === "Right" ||
        sort.column.TransitiveCount.t === "Left")
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(side: GraphSide): (idxs: NodeIDX[]) => Float32Array {
    const graph = (() => {
      switch (side) {
        case GRAPH_SIDE.L:
          return this.twinGraph.l;
        case GRAPH_SIDE.R:
          if (this.twinGraph.r == null) {
            throw new Error("Right graph is not available");
          }
          return this.twinGraph.r;
      }
    })();

    if (this.ctx.dominated) {
      return (idxs: NodeIDX[]) => graph.getTransitiveCountDominated(idxs);
    } else {
      return (idxs: NodeIDX[]) => graph.getTransitiveCount(idxs);
    }
  }

  getID(): string {
    return this.ctx.dominated ? "D(count) R" : "T(count) R";
  }

  definition(): [string, NumericValueColumnDefinition] {
    const getValues = this.getValuesFn(GRAPH_SIDE.R);
    const getValuesLeft = this.getValuesFn(GRAPH_SIDE.L);
    const columnID = this.getID();
    const r = this.twinGraph.r;
    if (r == null) {
      throw new Error("Right graph must be available");
    }

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (r.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <UHoverCard
              triggerClassname="w-full"
              content={
                <MetricDeltaRightHovercard
                  valueLeft={getValuesLeft([row.twinArrow.points_to])[0] ?? 0}
                  valueRight={getValues([row.twinArrow.points_to])[0] ?? 0}
                  format={NO_PRECISION_FORMAT}
                />
              }
            >
              <MetricCell
                value={getValues([row.twinArrow.points_to])[0] ?? 0}
                format={NO_PRECISION_FORMAT}
              />
            </UHoverCard>
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: getValues,
      sortable: this.sortable(),
      isHidden: false,
      hovercardContent: this.ctx.dominated ? (
        <TransitiveDominatedCountHovercard />
      ) : (
        <TransitiveCountHovercard />
      ),
    };
    return [columnID, definition];
  }
}

function TransitiveCountHovercard() {
  return (
    <div className="flex flex-col gap-2 p-2">
      <H1 text="Transitive Node Count" />
      <div>
        The total number of nodes reachable from this node, including itself.
      </div>
      <H2 text="Example:" />
      <Pre
        className="mt-2"
        text={`
        A
      /  \\
     B    C
      \\  /
        D

A = 4 (A, B, C, D)
B = 2 (B, D)
C = 2 (C, D)
D = 1 (D)
`}
      />
    </div>
  );
}

function TransitiveDominatedCountHovercard() {
  return (
    <div className="flex flex-col gap-2 p-2">
      <H1 text="Transitive Dominated Node Count" />
      <div>
        The total number of nodes that are dominated by this node, including
        itself.
      </div>
      <div>
        See{" "}
        <Link
          text="Dominator Trees"
          href="https://www.ngavalas.com/posts/dominator-trees"
          target="_blank"
        />
      </div>
      <div>
        In simpler words, dominated count means "how many nodes will become
        unreachable if this node is removed?"
      </div>
    </div>
  );
}

function MetricDeltaRightHovercard({
  valueLeft,
  valueRight,
  format,
}: {
  valueLeft: number;
  valueRight: number;
  format?: MetricFormat;
}) {
  return (
    <div className="flex flex-col gap-2 p-2">
      <table className="table-auto w-full">
        <tbody>
          <tr>
            <td className="text-left">Left (before)</td>
            <td className="text-right">
              <MetricCell value={valueLeft} format={format} />
            </td>
          </tr>
          <tr>
            <td className="text-left">Right (after)</td>
            <td className="text-right">
              <MetricCell value={valueRight} format={format} />
            </td>
          </tr>
          <tr>
            <td className="text-left">Delta</td>
            <td className="text-right font-semibold">
              <DeltaMetricCell value={valueRight - valueLeft} format={format} />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

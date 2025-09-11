// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { ColumnType } from "../../__generated__/ts/ColumnType";
import type { NodeIDX } from "../../__generated__/ts/NodeIDX";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../../ArrowUtils";

import NativeGraph, {
  GRAPH_SIDE,
  type GraphSide,
} from "../../native/NativeGraph";

import { H1, H2, Link, Pre } from "../../Typography";
import type { NumericValueColumnDefinition, TSortable } from "../TreeTable";
import type { Row } from "../TreeTableRows";
import {
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

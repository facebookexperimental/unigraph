// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "../../__generated__/ts/NodeIDX";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../../ArrowUtils";
import UHoverCard from "../../components/UHoverCard";
import ConjointCostDocs from "../../inline_docs/ConjointCost";
import NativeGraph, {
  GRAPH_SIDE,
  type GraphSide,
} from "../../native/NativeGraph";

import type TwinGraph from "../../native/TwinGraph";
import { H1, H2, Link, Pre } from "../../Typography";
import type { NumericValueColumnDefinition, TSortable } from "../columns";
import type { Row } from "../TreeTableRows";
import {
  DeltaMetricCell,
  MetricCell,
  MissingMetric,
  NO_PRECISION_FORMAT,
  WouldBeDeltaMetricCell,
} from "./Cells";
import { MV, isEnabledForGraphStructure, isViewVisible } from "./ColumnUtils";
import { MetricDeltaRightHovercard } from "./hovercards";
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
    return (
      isViewVisible(this.ctx.viewVisibility(MV.countTransitive)) &&
      this.ctx.showCounts
    );
  }

  sortable(): TSortable | null {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.countTransitive)
        : MV.countTransitive;
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          MetricView: { key },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "MetricView" in sort.column &&
      sort.column.MetricView.key === key
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float32Array {
    return (idxs: NodeIDX[]) => this.nativeGraph.getTransitiveCount(idxs);
  }

  getID(): string {
    const base = "T(count)";
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
      hovercardContent: <TransitiveCountHovercard />,
    };
    return [columnID, definition];
  }
}

export class DominatedCountColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  side: GraphSide | null;

  constructor(ctx: ColumnsCtx, nativeGraph: NativeGraph, side?: GraphSide) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.side = side ?? null;
  }

  isEnabled() {
    return (
      this.ctx.showCounts &&
      isEnabledForGraphStructure(
        this.ctx.graphStructure,
        this.ctx.viewVisibility(MV.countDominated),
      )
    );
  }

  sortable(): TSortable | null {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.countDominated)
        : MV.countDominated;
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          MetricView: { key },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "MetricView" in sort.column &&
      sort.column.MetricView.key === key
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float32Array {
    return (idxs: NodeIDX[]) =>
      this.nativeGraph.getTransitiveCountDominated(idxs);
  }

  getID(): string {
    const base = "D(count)";
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
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: getValues,
      sortable: this.sortable(),
      isHidden: false,
      hovercardContent: <TransitiveDominatedCountHovercard />,
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
    return (
      isViewVisible(this.ctx.viewVisibility(MV.countTransitive)) &&
      this.ctx.showCounts
    );
  }

  sortable(): TSortable | null {
    const key = MV.delta(MV.countTransitive);
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          MetricView: { key },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "MetricView" in sort.column &&
      sort.column.MetricView.key === key
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
      hovercardContent: <TransitiveCountHovercard />,
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
      isViewVisible(this.ctx.viewVisibility(MV.countTransitive)) &&
      this.ctx.showCounts &&
      this.twinGraph.l != null
    );
  }

  sortable(): TSortable | null {
    const key = MV.countTransitive;
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          MetricView: { key },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "MetricView" in sort.column &&
      sort.column.MetricView.key === key
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(side: GraphSide): (idxs: NodeIDX[]) => Float32Array {
    const graph = (() => {
      switch (side) {
        case GRAPH_SIDE.L:
          if (this.twinGraph.l == null) {
            throw new Error("Left graph is not available");
          }
          return this.twinGraph.l;
        case GRAPH_SIDE.R:
          return this.twinGraph.r;
      }
    })();

    return (idxs: NodeIDX[]) => graph.getTransitiveCount(idxs);
  }

  getID(): string {
    return "T(count) R";
  }

  definition(): [string, NumericValueColumnDefinition] {
    const getValues = this.getValuesFn(GRAPH_SIDE.R);
    const getValuesLeft = this.getValuesFn(GRAPH_SIDE.L);
    const columnID = this.getID();
    const r = this.twinGraph.r;

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
      hovercardContent: <TransitiveCountHovercard />,
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

export class ParentsCountColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  side: GraphSide | null;

  constructor(ctx: ColumnsCtx, nativeGraph: NativeGraph, side?: GraphSide) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.side = side ?? null;
  }

  isEnabled() {
    return (
      isViewVisible(this.ctx.viewVisibility(MV.parentsCount)) &&
      this.ctx.showCounts
    );
  }

  getID(): string {
    if (this.side == null) {
      return "Parents #";
    }
    return `Parents # ${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L ? MV.left(MV.parentsCount) : MV.parentsCount;
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          MetricView: { key },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "MetricView" in sort.column &&
      sort.column.MetricView.key === key
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={
                this.nativeGraph.getParentsCount([
                  row.twinArrow.points_to,
                ])[0] ?? 0
              }
              format={NO_PRECISION_FORMAT}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return this.nativeGraph.getParentsCount(idxs);
      },
      sortable: this.sortable(),
      isHidden: false,
    };
    return [columnID, definition];
  }
}

export class ConjointCountColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  side: GraphSide | null;

  constructor(ctx: ColumnsCtx, nativeGraph: NativeGraph, side?: GraphSide) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.side = side ?? null;
  }

  isEnabled() {
    return (
      isViewVisible(this.ctx.viewVisibility(MV.countConjoint)) &&
      this.ctx.showCounts
    );
  }

  getID(): string {
    if (this.side == null) {
      return "C(count)";
    }
    return `C(count)${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L ? MV.left(MV.countConjoint) : MV.countConjoint;
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          MetricView: { key },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "MetricView" in sort.column &&
      sort.column.MetricView.key === key
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={
                this.nativeGraph.getConjointCost().count[
                  row.twinArrow.points_to
                ] ?? 0
              }
              format={NO_PRECISION_FORMAT}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        const count = this.nativeGraph.getConjointCost().count;
        return new Float32Array(idxs.map((idx) => count[idx] ?? 0));
      },
      sortable: this.sortable(),
      isHidden: false,
      hovercardContent: <ConjointCostDocs />,
    };
    return [columnID, definition];
  }
}

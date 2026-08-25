// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import type { NodeIDX } from "../../__generated__/ts/NodeIDX";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../../ArrowUtils";
import UHoverCard from "../../components/UHoverCard";
import formatMetric from "../../lib/formatMetric";
import type NativeGraph from "../../native/NativeGraph";
import { GRAPH_SIDE, type GraphSide } from "../../native/NativeGraph";
import type TwinGraph from "../../native/TwinGraph";
import type { NumericValueColumnDefinition, TSortable } from "../columns";
import type { Row } from "../TreeTableRows";
import {
  DeltaMetricCell,
  MetricCell,
  MissingMetric,
  WouldBeDeltaMetricCell,
} from "./Cells";
import { MV } from "./ColumnUtils";
import { LazyMetricComparisonHovercard } from "./hovercards";
import type { Column, ColumnsCtx } from "./useGraphTreeTableColumns";

// ── Helpers ────────────────────────────────────────────────────

function sortableForView(ctx: ColumnsCtx, key: string): TSortable {
  const sortable: TSortable = {
    order: null,
    onSortChange: (order: SortOrder | null) =>
      ctx.onSortChange(order, { MetricView: { key } }),
  };

  const sort = ctx.sort();
  if (
    sort != null &&
    "MetricView" in sort.column &&
    sort.column.MetricView.key === key
  ) {
    sortable.order = sort.order;
  }

  return sortable;
}

// ── Self metric ────────────────────────────────────────────────

export class MetricColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  metricName: string;
  side: GraphSide | null;

  constructor(
    ctx: ColumnsCtx,
    nativeGraph: NativeGraph,
    metricName: string,
    side?: GraphSide,
  ) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.metricName = metricName;
    this.side = side ?? null;
  }

  isEnabled() {
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.metric(this.metricName))
    );
  }

  getID(): string {
    if (this.side == null) {
      return this.metricName;
    }
    return `${this.metricName} ${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.metric(this.metricName))
        : MV.metric(this.metricName);
    return sortableForView(this.ctx, key);
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const format = this.ctx.format(this.metricName);
    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      // A node's own metric value is intrinsic to the node, so we render it
      // even when the node is unreachable/excluded (unlike transitive/dominated
      // views, which sum over reachable descendants and stay `-`).
      renderer: (row: Readonly<Row>) => (
        <MetricCell
          value={this.nativeGraph.getNodeMetric(
            row.twinArrow.points_to,
            this.metricName,
          )}
          format={format}
          muted={!this.nativeGraph.isNodeReachable(row.twinArrow.points_to)}
        />
      ),
      getNumericValues: (idxs: NodeIDX[]) =>
        this.nativeGraph.getNodeMetricBatched(idxs, this.metricName),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Transitive metric ──────────────────────────────────────────

export class TransitiveMetricColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  metricName: string;
  side: GraphSide | null;

  constructor(
    ctx: ColumnsCtx,
    nativeGraph: NativeGraph,
    metricName: string,
    side?: GraphSide,
  ) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.metricName = metricName;
    this.side = side ?? null;
  }

  isEnabled() {
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.transitive(this.metricName))
    );
  }

  getID(): string {
    const base = `T(${this.metricName})`;

    if (this.side === GRAPH_SIDE.L) {
      return `${base} L`;
    } else if (this.side === GRAPH_SIDE.R) {
      return `${base} R`;
    }
    return base;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.transitive(this.metricName))
        : MV.transitive(this.metricName);
    return sortableForView(this.ctx, key);
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float64Array {
    return (idxs: NodeIDX[]) =>
      this.nativeGraph.getTransitiveMetricsBatched(idxs, this.metricName);
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={getValues([row.twinArrow.points_to])[0] as number}
              format={format}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: getValues,
      sortable: this.sortable(),
      isHidden: false,
    };
    return [columnID, definition];
  }
}

// ── Dominated metric ───────────────────────────────────────────

export class DominatedMetricColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  metricName: string;
  side: GraphSide | null;

  constructor(
    ctx: ColumnsCtx,
    nativeGraph: NativeGraph,
    metricName: string,
    side?: GraphSide,
  ) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.metricName = metricName;
    this.side = side ?? null;
  }

  isEnabled() {
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.dominated(this.metricName))
    );
  }

  getID(): string {
    const base = `D(${this.metricName})`;

    if (this.side === GRAPH_SIDE.L) {
      return `${base} L`;
    } else if (this.side === GRAPH_SIDE.R) {
      return `${base} R`;
    }
    return base;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.dominated(this.metricName))
        : MV.dominated(this.metricName);
    return sortableForView(this.ctx, key);
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float64Array {
    return (idxs: NodeIDX[]) =>
      this.nativeGraph.getTransitiveDominatedMetricsBatched(
        idxs,
        this.metricName,
      );
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={getValues([row.twinArrow.points_to])[0] as number}
              format={format}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: getValues,
      sortable: this.sortable(),
      isHidden: false,
    };
    return [columnID, definition];
  }
}

// ── Tiered transitive metric ───────────────────────────────────

export class TransitiveTieredMetricColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  metricName: string;
  tierName: string;
  side: GraphSide | null;

  constructor(
    ctx: ColumnsCtx,
    nativeGraph: NativeGraph,
    metricName: string,
    tierName: string,
    side?: GraphSide,
  ) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.metricName = metricName;
    this.tierName = tierName;
    this.side = side ?? null;
  }

  isEnabled() {
    const tierIDX = this.nativeGraph.stats().tier_names.indexOf(this.tierName);
    return (
      this.ctx.showMetrics &&
      this.ctx.isVisible(MV.tiered(this.metricName, this.tierName)) &&
      isBelowMaxTier(this.ctx, tierIDX)
    );
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.nativeGraph.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `${this.tierName} ${this.metricName}`
      : this.tierName;

    return base;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.tiered(this.metricName, this.tierName))
        : MV.tiered(this.metricName, this.tierName);
    return sortableForView(this.ctx, key);
  }

  getValuesFn(): (idxs: NodeIDX[]) => number[] {
    return (idxs: NodeIDX[]) => {
      return this.nativeGraph
        .getTieredTransitiveMetricsBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
    };
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={getValues([row.twinArrow.points_to])[0] as number}
              format={format}
            />
          );
        } else if (
          row.twinArrow.points_from === ARROW_POINTS_FROM_NON_EXISTENT
        ) {
          return <MissingMetric />;
        } else {
          const currentValue =
            this.nativeGraph.getCombinedMetricsForEntryPoints()
              .tiered_metrics?.[this.metricName]?.[this.tierName] ?? 0;
          const wouldBeValue =
            this.nativeGraph.getCombinedMetricsForEntryPointsWithOverrides({
              from: row.twinArrow.points_from,
              to: row.twinArrow.points_to,
            }).tiered_metrics?.[this.metricName]?.[this.tierName] ?? 0;
          return (
            <WouldBeDeltaMetricCell
              value={wouldBeValue - currentValue}
              format={format}
            />
          );
        }
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float64Array(getValues(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Tiered dominated metric ────────────────────────────────────

export class TieredDominatedMetricColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  metricName: string;
  tierName: string;
  side: GraphSide | null;

  constructor(
    ctx: ColumnsCtx,
    nativeGraph: NativeGraph,
    metricName: string,
    tierName: string,
    side?: GraphSide,
  ) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.metricName = metricName;
    this.tierName = tierName;
    this.side = side ?? null;
  }

  isEnabled() {
    const tierIDX = this.nativeGraph.stats().tier_names.indexOf(this.tierName);
    return (
      this.ctx.showMetrics &&
      this.ctx.isVisible(MV.tieredDominated(this.metricName, this.tierName)) &&
      isBelowMaxTier(this.ctx, tierIDX)
    );
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.nativeGraph.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `${this.tierName} ${this.metricName}`
      : this.tierName;

    return `D(${base})`;
  }

  sortable() {
    const key =
      this.side === GRAPH_SIDE.L
        ? MV.left(MV.tieredDominated(this.metricName, this.tierName))
        : MV.tieredDominated(this.metricName, this.tierName);
    return sortableForView(this.ctx, key);
  }

  getValuesFn(): (idxs: NodeIDX[]) => number[] {
    return (idxs: NodeIDX[]) => {
      return this.nativeGraph
        .getTieredTransitiveMetricsDominatedBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
    };
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={getValues([row.twinArrow.points_to])[0] as number}
              format={format}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float64Array(getValues(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Tiered transitive delta ────────────────────────────────────

export class TransitiveTieredMetricDeltaColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;
  tierName: string;

  constructor(
    ctx: ColumnsCtx,
    twinGraph: TwinGraph,
    metricName: string,
    tierName: string,
  ) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
    this.tierName = tierName;
  }

  isEnabled() {
    if (this.twinGraph.l == null) {
      return false;
    }
    const tierIDX = this.twinGraph.r.stats().tier_names.indexOf(this.tierName);

    return (
      this.ctx.showMetrics &&
      this.ctx.isVisible(MV.tiered(this.metricName, this.tierName)) &&
      isBelowMaxTier(this.ctx, tierIDX)
    );
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.twinGraph.r.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `∆ ${this.tierName} ${this.metricName}`
      : `∆ ${this.tierName}`;

    return base;
  }

  sortable() {
    return sortableForView(
      this.ctx,
      MV.delta(MV.tiered(this.metricName, this.tierName)),
    );
  }

  getValuesFn(): (idxs: NodeIDX[]) => number[] {
    return (idxs: NodeIDX[]) => {
      return this.twinGraph
        .getTieredTransitiveMetricsDeltaBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
    };
  }

  getValuesFnForSorting(): (idxs: NodeIDX[]) => Float64Array {
    return (idxs: NodeIDX[]) =>
      Float64Array.from(this.getValuesFn()(idxs).map((n) => Math.abs(n)));
  }

  /// Same shape as `TransitiveTieredMetricRightDeltaColumn.getValuesFn` — the
  /// per-side tiered values the `∆` is built from, which the hovercard shows.
  getSideValuesFn(side: GraphSide): (idxs: NodeIDX[]) => number[] {
    const g =
      side === GRAPH_SIDE.L ? this.twinGraph.leftGraphX() : this.twinGraph.r;

    return (idxs: NodeIDX[]) =>
      g
        .getTieredTransitiveMetricsBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const getValuesFnForSorting = this.getValuesFnForSorting();
    const getValuesL = this.getSideValuesFn(GRAPH_SIDE.L);
    const getValuesR = this.getSideValuesFn(GRAPH_SIDE.R);
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        return (
          <UHoverCard
            triggerClassname="w-full"
            asChild
            content={
              <LazyMetricComparisonHovercard
                getLeft={() => getValuesL([idx])[0] ?? 0}
                getRight={() => getValuesR([idx])[0] ?? 0}
                getDelta={() => getValues([idx])[0] ?? 0}
                format={format}
              />
            }
          >
            <DeltaMetricCell
              value={getValues([idx])[0] as number}
              format={format}
            />
          </UHoverCard>
        );
      },
      getNumericValues: getValuesFnForSorting,
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Tiered transitive right in delta view ──────────────────────

export class TransitiveTieredMetricRightDeltaColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;
  tierName: string;

  constructor(
    ctx: ColumnsCtx,
    twinGraph: TwinGraph,
    metricName: string,
    tierName: string,
  ) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
    this.tierName = tierName;
  }

  isEnabled() {
    if (this.twinGraph.l == null) {
      return false;
    }
    const tierIDX = this.twinGraph.r.stats().tier_names.indexOf(this.tierName);

    return (
      this.ctx.showMetrics &&
      this.ctx.isVisible(MV.tiered(this.metricName, this.tierName)) &&
      isBelowMaxTier(this.ctx, tierIDX)
    );
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.twinGraph.r.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `${this.tierName} ${this.metricName}`
      : this.tierName;

    return base;
  }

  sortable() {
    const key = MV.tiered(this.metricName, this.tierName);
    return sortableForView(this.ctx, key);
  }

  getValuesFn(side: GraphSide): (idxs: NodeIDX[]) => number[] {
    const g =
      side === GRAPH_SIDE.L ? this.twinGraph.leftGraphX() : this.twinGraph.r;

    return (idxs: NodeIDX[]) => {
      return g
        .getTieredTransitiveMetricsBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
    };
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValuesL = this.getValuesFn(GRAPH_SIDE.L);
    const getValuesR = this.getValuesFn(GRAPH_SIDE.R);
    const getDelta = (idxs: NodeIDX[]) =>
      this.twinGraph
        .getTieredTransitiveMetricsDeltaBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
    const format = this.ctx.format(this.metricName);
    const r = this.twinGraph.r;

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        return (
          <UHoverCard
            triggerClassname="w-full"
            asChild
            content={
              <LazyMetricComparisonHovercard
                getLeft={() => getValuesL([idx])[0] ?? 0}
                getRight={() => getValuesR([idx])[0] ?? 0}
                getDelta={() => getDelta([idx])[0] ?? 0}
                format={format}
              />
            }
          >
            {r.isNodeReachable(idx) ? (
              <MetricCell
                value={getValuesR([idx])[0] as number}
                format={format}
              />
            ) : (
              <MissingMetric />
            )}
          </UHoverCard>
        );
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float64Array(getValuesR(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Self metric right in delta view ────────────────────────────

export class MetricRightInDeltaViewColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph, metricName: string) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
  }

  isEnabled() {
    if (this.twinGraph.l == null) {
      return false;
    }
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.metric(this.metricName))
    );
  }

  getID(): string {
    return `${this.metricName} R`;
  }

  sortable() {
    return sortableForView(this.ctx, MV.metric(this.metricName));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const r = this.twinGraph.r;
    const columnID = this.getID();
    const getValuesL = (idxs: NodeIDX[]) =>
      this.twinGraph.leftGraphX().getNodeMetricBatched(idxs, this.metricName);
    const getValuesR = (idxs: NodeIDX[]) =>
      r.getNodeMetricBatched(idxs, this.metricName);
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        return (
          <UHoverCard
            triggerClassname="w-full"
            asChild
            content={
              <LazyMetricComparisonHovercard
                getLeft={() => getValuesL([idx])[0] ?? 0}
                getRight={() => getValuesR([idx])[0] ?? 0}
                format={format}
              />
            }
          >
            {r.isNodeReachable(idx) ? (
              <MetricCell
                value={r.getNodeMetric(idx, this.metricName)}
                format={format}
              />
            ) : (
              <MissingMetric />
            )}
          </UHoverCard>
        );
      },
      getNumericValues: (idxs: NodeIDX[]) =>
        r.getNodeMetricBatched(idxs, this.metricName),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Self metric delta ──────────────────────────────────────────

export class MetricDeltaViewColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph, metricName: string) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
  }

  isEnabled() {
    if (this.twinGraph.l == null) {
      return false;
    }
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.metric(this.metricName))
    );
  }

  getID(): string {
    return `∆(${this.metricName})`;
  }

  sortable() {
    return sortableForView(this.ctx, MV.delta(MV.metric(this.metricName)));
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float64Array {
    const r = this.twinGraph.r;
    const l = this.twinGraph.leftGraphX();
    return (idxs: NodeIDX[]) => {
      const valuesL = l.getNodeMetricBatched(idxs, this.metricName);
      const valuesR = r.getNodeMetricBatched(idxs, this.metricName);

      const deltas = new Float64Array(idxs.length);
      for (let i = 0; i < idxs.length; i++) {
        deltas[i] = (valuesR[i] ?? 0) - (valuesL[i] ?? 0);
      }

      return deltas;
    };
  }

  getValuesFnForSorting(): (idxs: NodeIDX[]) => Float64Array {
    return (idxs: NodeIDX[]) =>
      this.getValuesFn()(idxs).map((n) => Math.abs(n));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const r = this.twinGraph.r;
    const l = this.twinGraph.leftGraphX();
    const columnID = this.getID();
    const format = this.ctx.format(this.metricName);
    const getValues = this.getValuesFn();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        return (
          <UHoverCard
            triggerClassname="w-full"
            asChild
            content={
              <LazyMetricComparisonHovercard
                getLeft={() =>
                  l.getNodeMetricBatched([idx], this.metricName)[0] ?? 0
                }
                getRight={() =>
                  r.getNodeMetricBatched([idx], this.metricName)[0] ?? 0
                }
                format={format}
              />
            }
          >
            {r.isNodeReachable(idx) ? (
              <DeltaMetricCell
                value={getValues([idx])[0] ?? 0}
                format={format}
              />
            ) : (
              <MissingMetric />
            )}
          </UHoverCard>
        );
      },
      getNumericValues: this.getValuesFnForSorting(),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Transitive metric right in delta view ──────────────────────

export class TransitiveMetricRightInDeltaViewColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph, metricName: string) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
  }

  isEnabled() {
    if (this.twinGraph.l == null) {
      return false;
    }
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.transitive(this.metricName))
    );
  }

  getID(): string {
    return `T(${this.metricName}) R`;
  }

  sortable() {
    return sortableForView(this.ctx, MV.transitive(this.metricName));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const r = this.twinGraph.r;
    const columnID = this.getID();
    const getValuesL = (idxs: NodeIDX[]) =>
      this.twinGraph
        .leftGraphX()
        .getTransitiveMetricsBatched(idxs, this.metricName);
    const getValuesR = (idxs: NodeIDX[]) =>
      r.getTransitiveMetricsBatched(idxs, this.metricName);
    const format = this.ctx.format(this.metricName);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        return (
          <UHoverCard
            triggerClassname="w-full"
            asChild
            content={
              <LazyMetricComparisonHovercard
                getLeft={() => getValuesL([idx])[0] ?? 0}
                getRight={() => getValuesR([idx])[0] ?? 0}
                format={format}
              />
            }
          >
            {r.isNodeReachable(idx) ? (
              <MetricCell
                value={getValuesR([idx])[0] as number}
                format={format}
              />
            ) : (
              <MissingMetric />
            )}
          </UHoverCard>
        );
      },
      getNumericValues: (idxs: NodeIDX[]) => getValuesR(idxs),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Transitive metric delta ────────────────────────────────────

export class TransitiveMetricDeltaColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph, metricName: string) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
  }

  isEnabled() {
    if (this.twinGraph.l == null) {
      return false;
    }
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.transitive(this.metricName))
    );
  }

  getID(): string {
    return `∆T(${this.metricName})`;
  }

  sortable() {
    return sortableForView(this.ctx, MV.delta(MV.transitive(this.metricName)));
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float64Array {
    const r = this.twinGraph.r;
    const l = this.twinGraph.leftGraphX();
    return (idxs: NodeIDX[]) => {
      const valuesL = l.getTransitiveMetricsBatched(idxs, this.metricName);
      const valuesR = r.getTransitiveMetricsBatched(idxs, this.metricName);

      const deltas = new Float64Array(idxs.length);
      for (let i = 0; i < idxs.length; i++) {
        deltas[i] = (valuesR[i] ?? 0) - (valuesL[i] ?? 0);
      }

      return deltas;
    };
  }

  getValuesFnForSorting(): (idxs: NodeIDX[]) => Float64Array {
    return (idxs: NodeIDX[]) =>
      this.getValuesFn()(idxs).map((n) => Math.abs(n));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const r = this.twinGraph.r;
    const l = this.twinGraph.leftGraphX();
    const columnID = this.getID();
    const format = this.ctx.format(this.metricName);
    const getValues = this.getValuesFn();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        return (
          <UHoverCard
            triggerClassname="w-full"
            asChild
            content={
              <LazyMetricComparisonHovercard
                getLeft={() =>
                  l.getTransitiveMetricsBatched([idx], this.metricName)[0] ?? 0
                }
                getRight={() =>
                  r.getTransitiveMetricsBatched([idx], this.metricName)[0] ?? 0
                }
                format={format}
              />
            }
          >
            {r.isNodeReachable(idx) ? (
              <DeltaMetricCell
                value={getValues([idx])[0] ?? 0}
                format={format}
              />
            ) : (
              <MissingMetric />
            )}
          </UHoverCard>
        );
      },
      getNumericValues: this.getValuesFnForSorting(),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Enum metric ────────────────────────────────────────────────
// A numeric metric formatted as an enum is categorical: summing it over
// descendants (transitive/dominated/tiered) is meaningless. So it collapses
// to a single column that shows the node's own label, mirroring NodeTierColumn.
// In delta mode: same value on both sides → one label; different → `L ► R`.

export class EnumMetricColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph, metricName: string) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
  }

  isEnabled() {
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.metric(this.metricName))
    );
  }

  getID(): string {
    return this.metricName;
  }

  sortable() {
    return sortableForView(this.ctx, MV.metric(this.metricName));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const left = this.twinGraph.l;
    const right = this.twinGraph.r;
    const format = this.ctx.format(this.metricName);
    const columnID = this.getID();

    const labelFor = (g: NativeGraph, idx: NodeIDX): string | null =>
      g.isNodeReachable(idx)
        ? formatMetric(g.getNodeMetric(idx, this.metricName), format)
        : null;

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        const labelR = labelFor(right, idx);

        if (left == null || labelFor(left, idx) === labelR) {
          return labelR == null ? (
            <MissingMetric />
          ) : (
            <div className="flex justify-center w-full">
              <EnumBadge label={labelR} />
            </div>
          );
        }

        const labelL = labelFor(left, idx);
        return (
          <div className="flex justify-center w-full">
            <EnumBadge label={labelL ?? "-"} />
            <span className="text-[10px] self-center px-1">►</span>
            <EnumBadge label={labelR ?? "-"} />
          </div>
        );
      },
      // Sorts by the underlying numeric value (right graph in delta mode),
      // even though the display is categorical.
      getNumericValues: (idxs: NodeIDX[]) =>
        right.getNodeMetricBatched(idxs, this.metricName),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

function EnumBadge({ label }: { label: string }) {
  return (
    <span className="text-xs py-0.5 px-2 rounded-lg border border-accent-foreground/30 bg-graph-500/50">
      {label}
    </span>
  );
}

// ── Timespan metric ─────────────────────────────────────────────
// A metric marked as a timespan START holds one end of a span; the paired END
// value lives in another metric. Instead of a number we render a horizontal
// bar positioned along a timeline shared by every row: the timeline spans
// min(start) .. max(end) across ALL nodes. Aggregating a timestamp is
// meaningless, so (like enum) this collapses to a single column. Single-graph
// only — delta mode never instantiates this column.

export class TimespanMetricColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;
  metricName: string;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph, metricName: string) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
    this.metricName = metricName;
  }

  isEnabled() {
    return (
      this.ctx.showMetrics && this.ctx.isVisible(MV.metric(this.metricName))
    );
  }

  getID(): string {
    return this.metricName;
  }

  sortable() {
    return sortableForView(this.ctx, MV.metric(this.metricName));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const g = this.twinGraph.r;
    const columnID = this.getID();
    const startMetric = this.metricName;
    const startFormat = this.ctx.format(startMetric);
    const tsConfig = this.ctx.timespanConfig(startMetric);
    const endMetric = tsConfig?.endMetricName ?? null;
    const ignoreZero = tsConfig?.ignoreZero ?? false;
    const endFormat =
      endMetric != null ? this.ctx.format(endMetric) : undefined;

    // Shared timeline: earliest start .. latest end across ALL nodes.
    // Memoized in NativeGraph, so this is computed once per column, not per row.
    const startRange = g.getMetricMinMax(startMetric, ignoreZero);
    const endRange =
      endMetric != null ? g.getMetricMinMax(endMetric, ignoreZero) : null;
    const min = startRange?.min ?? 0;
    const max = endRange?.max ?? startRange?.max ?? 0;
    const range = max - min;

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      // The span is intrinsic to the node, so render the bar even when the
      // node is unreachable/excluded.
      renderer: (row: Readonly<Row>) => {
        const idx = row.twinArrow.points_to;
        const start = g.getNodeMetric(idx, startMetric);
        const end = endMetric != null ? g.getNodeMetric(idx, endMetric) : start;
        const muted = !g.isNodeReachable(idx);

        // With ignore_zero, a node whose start and end are both the default
        // 0.0 has no real span — render only the timeline ruler, no bar.
        const hasSpan = !(ignoreZero && start === 0 && end === 0);

        const leftPct = range > 0 ? ((start - min) / range) * 100 : 0;
        const widthPct = range > 0 ? ((end - start) / range) * 100 : 0;
        const clampedLeft = Math.max(0, Math.min(100, leftPct));
        const clampedWidth = Math.max(0, Math.min(100 - clampedLeft, widthPct));

        const tooltip = `${formatMetric(start, startFormat)} → ${formatMetric(
          end,
          endFormat,
        )}`;

        return (
          <div
            className="relative w-full h-3"
            title={hasSpan ? tooltip : undefined}
          >
            {/* Timeline edge ticks: left = earliest start, right = latest end. */}
            <div className="absolute left-0 inset-y-0 w-px bg-border" />
            <div className="absolute right-0 inset-y-0 w-px bg-border" />
            {hasSpan && (
              <div
                className={clsx(
                  "absolute top-1/2 -translate-y-1/2 bg-primary rounded-md h-2",
                  muted && "opacity-40",
                )}
                style={{
                  left: `${clampedLeft}%`,
                  width: `${clampedWidth}%`,
                  minWidth: "2px",
                }}
              />
            )}
          </div>
        );
      },
      // Sort by the span's start value.
      getNumericValues: (idxs: NodeIDX[]) =>
        g.getNodeMetricBatched(idxs, startMetric),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

// ── Helpers ────────────────────────────────────────────────────

function isBelowMaxTier(ctx: ColumnsCtx, tierIDX: number): boolean {
  if (tierIDX == null) {
    return true;
  }

  const maxTier = ctx.tvc.tiered_traversal?.AscendingTiers.max_tier;
  if (maxTier == null) {
    return true;
  }

  if (tierIDX > maxTier) {
    return false;
  }
  return true;
}

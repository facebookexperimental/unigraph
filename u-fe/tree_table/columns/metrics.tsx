// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { ColumnType } from "../../__generated__/ts/ColumnType";
import type { NodeIDX } from "../../__generated__/ts/NodeIDX";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../../ArrowUtils";
import UHoverCard from "../../components/UHoverCard";
import type NativeGraph from "../../native/NativeGraph";
import { GRAPH_SIDE, type GraphSide } from "../../native/NativeGraph";
import type TwinGraph from "../../native/TwinGraph";
import type { NumericValueColumnDefinition, TSortable } from "../TreeTable";
import type { Row } from "../TreeTableRows";
import {
  DeltaMetricCell,
  MetricCell,
  MissingMetric,
  WouldBeDeltaMetricCell,
} from "./Cells";
import { MetricDeltaRightHovercard } from "./hovercards";
import type { Column, ColumnsCtx } from "./useGraphTreeTableColumns";

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
      this.ctx.showMetrics &&
      this.ctx.metricSettings(this.metricName)?.column_hide_self !== true
    );
  }

  getID(): string {
    if (this.side == null) {
      return this.metricName;
    }
    return `${this.metricName} ${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          Metric: {
            t: columnType,
            name: this.metricName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "Metric" in sort.column &&
      sort.column.Metric.t === columnType &&
      sort.column.Metric.name === this.metricName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const metricSettings = this.ctx.metricSettings(this.metricName);
    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (this.nativeGraph.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <MetricCell
              value={this.nativeGraph.getNodeMetric(
                row.twinArrow.points_to,
                this.metricName,
              )}
              format={metricSettings?.format}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: (idxs: NodeIDX[]) =>
        this.nativeGraph.getNodeMetricBatched(idxs, this.metricName),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

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
      this.ctx.showTransitive &&
      this.ctx.metricSettings(this.metricName)?.column_show_transitive ===
        "WhenEnabledGlobally"
    );
  }

  getID(): string {
    const base = this.ctx.dominated
      ? `D(${this.metricName})`
      : `T(${this.metricName})`;

    if (this.side === GRAPH_SIDE.L) {
      return `${base} L`;
    } else if (this.side === GRAPH_SIDE.R) {
      return `${base} R`;
    }
    return base;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TransitiveMetric: {
            t: columnType,
            name: this.metricName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TransitiveMetric" in sort.column &&
      sort.column.TransitiveMetric.t === columnType &&
      sort.column.TransitiveMetric.name === this.metricName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float32Array {
    if (this.ctx.dominated) {
      return (idxs: NodeIDX[]) =>
        this.nativeGraph.getTransitiveDominatedMetricsBatched(
          idxs,
          this.metricName,
        );
    } else {
      return (idxs: NodeIDX[]) =>
        this.nativeGraph.getTransitiveMetricsBatched(idxs, this.metricName);
    }
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const format = this.ctx.metricSettings(this.metricName)?.format;

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
    return isTierEnabled(this.ctx, this.metricName, this.tierName, tierIDX);
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.nativeGraph.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `${this.tierName} ${this.metricName}`
      : this.tierName;

    if (this.ctx.dominated) {
      return `D(${base})`;
    }
    return base;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TieredTransitiveMetric: {
            t: columnType,
            name: this.metricName,
            tier: this.tierName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TieredTransitiveMetric" in sort.column &&
      sort.column.TieredTransitiveMetric.t === columnType &&
      sort.column.TieredTransitiveMetric.name === this.metricName &&
      sort.column.TieredTransitiveMetric.tier === this.tierName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => number[] {
    if (this.ctx.dominated) {
      return (idxs: NodeIDX[]) => {
        return this.nativeGraph
          .getTieredTransitiveMetricsDominatedBatched(idxs, this.metricName)
          .map((m) => m[this.tierName] ?? 0);
      };
    } else {
      return (idxs: NodeIDX[]) => {
        return this.nativeGraph
          .getTieredTransitiveMetricsBatched(idxs, this.metricName)
          .map((m) => m[this.tierName] ?? 0);
      };
    }
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const format = this.ctx.metricSettings(this.metricName)?.format;

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
          // If the arrow is coming from a non-existent point, we don't know
          // what to override
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
        return new Float32Array(getValues(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

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
    if (this.twinGraph.r == null) {
      return false;
    }
    const tierIDX = this.twinGraph.r.stats().tier_names.indexOf(this.tierName);
    return isTierEnabled(this.ctx, this.metricName, this.tierName, tierIDX);
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.twinGraph.l.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `∆ ${this.tierName} ${this.metricName}`
      : `∆ ${this.tierName}`;

    if (this.ctx.dominated) {
      return `D(${base})`;
    }
    return base;
  }

  sortable() {
    const columnType: ColumnType = "Delta";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TieredTransitiveMetric: {
            t: columnType,
            name: this.metricName,
            tier: this.tierName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TieredTransitiveMetric" in sort.column &&
      sort.column.TieredTransitiveMetric.t === columnType &&
      sort.column.TieredTransitiveMetric.name === this.metricName &&
      sort.column.TieredTransitiveMetric.tier === this.tierName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => number[] {
    return (idxs: NodeIDX[]) => {
      return this.twinGraph
        .getTieredTransitiveMetricsDeltaBatched(idxs, this.metricName)
        .map((m) => m[this.tierName] ?? 0);
    };
  }

  getValuesFnForSorting(): (idxs: NodeIDX[]) => Float32Array {
    return (idxs: NodeIDX[]) =>
      Float32Array.from(this.getValuesFn()(idxs).map((n) => Math.abs(n)));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValues = this.getValuesFn();
    const getValuesFnForSorting = this.getValuesFnForSorting();
    const format = this.ctx.metricSettings(this.metricName)?.format;

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        return (
          <DeltaMetricCell
            value={getValues([row.twinArrow.points_to])[0] as number}
            format={format}
          />
        );
      },
      getNumericValues: getValuesFnForSorting,
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

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
    if (this.twinGraph.r == null) {
      return false;
    }
    const tierIDX = this.twinGraph.r.stats().tier_names.indexOf(this.tierName);
    return isTierEnabled(this.ctx, this.metricName, this.tierName, tierIDX);
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.twinGraph.l.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `${this.tierName} ${this.metricName}`
      : this.tierName;

    if (this.ctx.dominated) {
      return `D(${base})`;
    }
    return base;
  }

  sortable() {
    const columnType: ColumnType = "Right";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          TieredTransitiveMetric: {
            t: columnType,
            name: this.metricName,
            tier: this.tierName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "TieredTransitiveMetric" in sort.column &&
      (sort.column.TieredTransitiveMetric.t === "Right" ||
        sort.column.TieredTransitiveMetric.t === "Left") &&
      sort.column.TieredTransitiveMetric.name === this.metricName &&
      sort.column.TieredTransitiveMetric.tier === this.tierName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(side: GraphSide): (idxs: NodeIDX[]) => number[] {
    const g =
      side === GRAPH_SIDE.L ? this.twinGraph.l : this.twinGraph.rightGraphX();
    if (this.ctx.dominated) {
      return (idxs: NodeIDX[]) => {
        return g
          .getTieredTransitiveMetricsDominatedBatched(idxs, this.metricName)
          .map((m) => m[this.tierName] ?? 0);
      };
    } else {
      return (idxs: NodeIDX[]) => {
        return g
          .getTieredTransitiveMetricsBatched(idxs, this.metricName)
          .map((m) => m[this.tierName] ?? 0);
      };
    }
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const getValuesL = this.getValuesFn(GRAPH_SIDE.L);
    const getValuesR = this.getValuesFn(GRAPH_SIDE.R);
    const format = this.ctx.metricSettings(this.metricName)?.format;
    const r = this.twinGraph.rightGraphX();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        return (
          <UHoverCard
            triggerClassname="w-full"
            content={
              <MetricDeltaRightHovercard
                valueLeft={getValuesL([row.twinArrow.points_to])[0] ?? 0}
                valueRight={getValuesR([row.twinArrow.points_to])[0] ?? 0}
                format={format}
              />
            }
          >
            {r.isNodeReachable(row.twinArrow.points_to) ? (
              <MetricCell
                value={getValuesR([row.twinArrow.points_to])[0] as number}
                format={format}
              />
            ) : (
              <MissingMetric />
            )}
          </UHoverCard>
        );
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(getValuesR(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

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
    if (this.twinGraph.r == null) {
      return false;
    }
    return (
      this.ctx.showMetrics &&
      this.ctx.metricSettings(this.metricName)?.column_hide_self !== true
    );
  }

  getID(): string {
    return `${this.metricName} R`;
  }

  sortable() {
    const columnType: ColumnType = "Right";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          Metric: {
            t: columnType,
            name: this.metricName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "Metric" in sort.column &&
      (sort.column.Metric.t === "Left" || sort.column.Metric.t === "Right") &&
      sort.column.Metric.name === this.metricName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const r = this.twinGraph.rightGraphX();
    const columnID = this.getID();
    const getValuesL = (idxs: NodeIDX[]) =>
      this.twinGraph.l.getNodeMetricBatched(idxs, this.metricName);
    const getValuesR = (idxs: NodeIDX[]) =>
      r.getNodeMetricBatched(idxs, this.metricName);
    const metricSettings = this.ctx.metricSettings(this.metricName);
    const format = metricSettings?.format;

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
                  valueLeft={getValuesL([row.twinArrow.points_to])[0] ?? 0}
                  valueRight={getValuesR([row.twinArrow.points_to])[0] ?? 0}
                  format={format}
                />
              }
            >
              <MetricCell
                value={r.getNodeMetric(
                  row.twinArrow.points_to,
                  this.metricName,
                )}
                format={format}
              />
            </UHoverCard>
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: (idxs: NodeIDX[]) =>
        r.getNodeMetricBatched(idxs, this.metricName),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

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
    if (this.twinGraph.r == null) {
      return false;
    }
    return (
      this.ctx.showMetrics &&
      this.ctx.metricSettings(this.metricName)?.column_hide_self !== true
    );
  }

  getID(): string {
    return `∆(${this.metricName})`;
  }

  sortable() {
    const columnType: ColumnType = "Delta";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          Metric: {
            t: columnType,
            name: this.metricName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "Metric" in sort.column &&
      sort.column.Metric.t === columnType &&
      sort.column.Metric.name === this.metricName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  getValuesFn(): (idxs: NodeIDX[]) => Float32Array {
    const r = this.twinGraph.rightGraphX();
    const l = this.twinGraph.l;
    return (idxs: NodeIDX[]) => {
      const valuesL = l.getNodeMetricBatched(idxs, this.metricName);
      const valuesR = r.getNodeMetricBatched(idxs, this.metricName);

      const deltas = new Float32Array(idxs.length);
      for (let i = 0; i < idxs.length; i++) {
        deltas[i] = (valuesR[i] ?? 0) - (valuesL[i] ?? 0);
      }

      return deltas;
    };
  }

  getValuesFnForSorting(): (idxs: NodeIDX[]) => Float32Array {
    return (idxs: NodeIDX[]) =>
      this.getValuesFn()(idxs).map((n) => Math.abs(n));
  }

  definition(): [string, NumericValueColumnDefinition] {
    const r = this.twinGraph.rightGraphX();
    const columnID = this.getID();
    const metricSettings = this.ctx.metricSettings(this.metricName);
    const getValues = this.getValuesFn();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        if (r.isNodeReachable(row.twinArrow.points_to)) {
          return (
            <DeltaMetricCell
              value={getValues([row.twinArrow.points_to])[0] ?? 0}
              format={metricSettings?.format}
            />
          );
        } else {
          return <MissingMetric />;
        }
      },
      getNumericValues: this.getValuesFnForSorting(),
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

function isTierEnabled(
  ctx: ColumnsCtx,
  metricName: string,
  tierName: string,
  tierIDX: number,
) {
  const metricSettings = ctx.metricSettings(metricName);

  if (tierIDX == null) {
    return false;
  }

  const maxTier = ctx.tvc.tiered_traversal?.AscendingTiers.max_tier;

  if (metricSettings?.column_show_tiered == null && maxTier != null) {
    if (tierIDX > maxTier) {
      return false;
    }
  }

  if (metricSettings?.column_show_tiered?.[tierName] === "Never") {
    return false;
  }
  return ctx.graphSettings.ui_settings?.columns?.show_tiered === true;
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { ColumnType } from "../../__generated__/ts/ColumnType";
import type { NodeIDX } from "../../__generated__/ts/NodeIDX";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../../ArrowUtils";
import type NativeGraph from "../../native/NativeGraph";
import { GRAPH_SIDE, type GraphSide } from "../../native/NativeGraph";
import type { NumericValueColumnDefinition, TSortable } from "../TreeTable";
import type { Row } from "../TreeTableRows";
import { MetricCell, MissingMetric, WouldBeDeltaMetricCell } from "./Cells";
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
    const metricSettings = this.ctx.metricSettings(this.metricName);
    const tierIDX = this.nativeGraph.stats().tier_names.indexOf(this.tierName);

    if (tierIDX == null) {
      return false;
    }

    const maxTier = this.ctx.tvc.tiered_traversal?.AscendingTiers.max_tier;

    if (metricSettings?.column_show_tiered == null && maxTier != null) {
      if (tierIDX > maxTier) {
        return false;
      }
    }

    if (metricSettings?.column_show_tiered?.[this.tierName] === "Never") {
      return false;
    }
    return this.ctx.graphSettings.ui_settings?.columns?.show_tiered === true;
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

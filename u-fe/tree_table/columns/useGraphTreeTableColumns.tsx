// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useMemo } from "react";
import type { ColumnType } from "../../__generated__/ts/ColumnType";
import type { GraphSettings } from "../../__generated__/ts/GraphSettings";
import type { GraphTableSort } from "../../__generated__/ts/GraphTableSort";
import type { MetricSettings } from "../../__generated__/ts/MetricSettings";
import type { SortColumn } from "../../__generated__/ts/SortColumn";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import type { TraversalConfig } from "../../__generated__/ts/TraversalConfig";
import { useGraphSettings } from "../../context/GraphSettingsContext";
import { useTwinGraph } from "../../context/NativeGraphContext";
import { useTVC } from "../../context/TraversalConfigContext";
import ConjointCostDocs from "../../inline_docs/ConjointCost";
import type NativeGraph from "../../native/NativeGraph";
import { GRAPH_SIDE, type GraphSide } from "../../native/NativeGraph";
import type TwinGraph from "../../native/TwinGraph";
import type { NodeIDX } from "../../types";
import ContextMenuCell from "../ContextMenuCell";
import type {
  ColumnDefinitions,
  ColumnID,
  NonTreeColumnDefinition,
  NumericValueColumnDefinition,
  TreeColumnDefinition,
  TSortable,
} from "../columns";
import type { Row } from "../TreeTableRows";
import { MetricCell, MissingMetric, NO_PRECISION_FORMAT } from "./Cells";
import {
  MetricColumn,
  MetricDeltaViewColumn,
  MetricRightInDeltaViewColumn,
  TransitiveMetricColumn,
  TransitiveTieredMetricColumn,
  TransitiveTieredMetricDeltaColumn,
  TransitiveTieredMetricRightDeltaColumn,
} from "./metrics";
import { NodeTierColumn } from "./tiers";
import {
  TransitiveCountColumn,
  TransitiveCountDeltaColumn,
  TransitiveCountRightInDeltaViewColumn,
} from "./transitiveCounts";

export interface Column {
  isEnabled: () => boolean;
  definition: () => [string, NonTreeColumnDefinition];
  sortable: () => TSortable | null;
}

export default function useGraphTreeTableColumns(): ColumnDefinitions {
  const twinGraph = useTwinGraph();
  const l = twinGraph.l;
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const { tvcL: tvc } = useTVC();

  return useMemo(() => {
    const builder =
      twinGraph.r !== null
        ? new DeltaGraphColumnsBuilder(
            twinGraph,
            graphSettings,
            setGraphSettings,
            tvc,
          )
        : new SingleGraphColumnsBuilder(
            twinGraph,
            graphSettings,
            setGraphSettings,
            tvc,
          );

    const nonTreeColumns: { [columnID: ColumnID]: NonTreeColumnDefinition } =
      {};

    for (const column of builder.makeColumns()) {
      if (column.isEnabled()) {
        const [id, def] = column.definition();
        nonTreeColumns[id] = def;
      }
    }

    const nodeNameSortOrder = (() => {
      const tableSort =
        graphSettings?.ui_settings?.columns?.graph_table_sort ?? null;
      if (tableSort == null) {
        return null;
      }

      if ("NodeName" in tableSort.column) {
        return tableSort.order;
      }

      return null;
    })();

    const treeColumn: TreeColumnDefinition = {
      label: "Node Name",
      getNodeName: (idx: NodeIDX) => l.getNodeName(idx),
      flexGrow: 1,
      sortable: {
        order: nodeNameSortOrder,
        onSortChange: (order: SortOrder | null) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings?.ui_settings?.columns,
                graph_table_sort:
                  order == null
                    ? undefined
                    : {
                        order,
                        column: { NodeName: {} },
                      },
              },
            },
          });
        },
      },
    };

    if (twinGraph.r === null) {
      // only add the `...` column when it's a single graph.
      // We should have something eventually for delta graph, but
      // it'll require some thought on what actually goes there.
      // and which graph these actions should apply to.
      nonTreeColumns.context_menu = {
        t: "non_sortable_column",
        label: "More Menu",
        renderer: (row: Readonly<Row>) => <ContextMenuCell row={row} />,
        isHidden: false,
        isLabelHidden: true,
      };
    }

    return {
      treeColumn,
      columns: nonTreeColumns,
    };
  }, [twinGraph, l, graphSettings, setGraphSettings, tvc]);
}

/// Simple context type to capture the current settings on the graph
/// with consistent defaults.
export class ColumnsCtx {
  graphSettings: GraphSettings;
  setGraphSettings: (gs: GraphSettings) => void;
  tvc: TraversalConfig;
  showTransitive: boolean;
  showTransitiveCount: boolean;
  showParentsCount: boolean;
  dominated: boolean;
  showMetrics: boolean;
  showTiered: boolean;
  showConjoint: boolean;
  showConjointCount: boolean;

  constructor(
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
  ) {
    this.graphSettings = graphSettings;
    this.setGraphSettings = setGraphSettings;
    this.tvc = tvc;

    this.showTransitive =
      graphSettings.ui_settings?.columns?.show_transitive ?? false;
    this.showTransitiveCount =
      graphSettings.ui_settings?.columns?.show_transitive_count !== "Never";
    this.showParentsCount =
      graphSettings.ui_settings?.columns?.show_parents_count === true;
    this.dominated = graphSettings.ui_settings?.graph_structure === "Dominator";
    this.showMetrics =
      graphSettings.ui_settings?.columns?.hide_metrics !== true;
    this.showTiered = graphSettings.ui_settings?.columns?.show_tiered === true;
    this.showConjoint =
      graphSettings.ui_settings?.columns?.show_conjoint ?? false;
    this.showConjointCount =
      graphSettings.ui_settings?.columns?.show_conjoint_count !== "Never";
  }

  metricSettings(metricName: string): MetricSettings | null {
    return (
      this.graphSettings.ui_settings?.columns?.metric_settings?.[metricName] ??
      null
    );
  }

  sort(): GraphTableSort | null {
    return this.graphSettings.ui_settings?.columns?.graph_table_sort ?? null;
  }

  onSortChange(order: SortOrder | null, column: SortColumn) {
    this.setGraphSettings({
      ...this.graphSettings,
      ui_settings: {
        ...this.graphSettings.ui_settings,
        columns: {
          ...this.graphSettings?.ui_settings?.columns,
          graph_table_sort: order == null ? undefined : { column, order },
        },
      },
    });
  }
}

class SingleGraphColumnsBuilder {
  twinGraph: TwinGraph;
  graphSettings: GraphSettings;
  setGraphSettings: (gs: GraphSettings) => void;
  tvc: TraversalConfig;
  ctx: ColumnsCtx;
  columns: Column[] = [];

  constructor(
    twinGraph: TwinGraph,
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
  ) {
    this.twinGraph = twinGraph;
    this.graphSettings = graphSettings;
    this.setGraphSettings = setGraphSettings;
    this.tvc = tvc;
    this.ctx = new ColumnsCtx(graphSettings, setGraphSettings, tvc);
    this.columns = [];
  }

  makeColumns(): Column[] {
    const { ctx, twinGraph } = this;
    const g = twinGraph.l;
    const columns: Column[] = [
      new NodeTierColumn(this.ctx, this.twinGraph),
      new TransitiveCountColumn(ctx, g),
      new ConjointCountColumn(ctx, g),
      new ParentsCountColumn(ctx, g),
    ];

    for (const metric of g.metricNames) {
      columns.push(new MetricColumn(ctx, g, metric));
      columns.push(new TransitiveMetricColumn(ctx, g, metric));
      columns.push(new ConjointMetricColumn(ctx, g, metric));

      for (const tier of g.stats().tier_names) {
        columns.push(new TransitiveTieredMetricColumn(ctx, g, metric, tier));
        columns.push(new ConjointTieredMetricColumn(ctx, g, metric, tier));
      }
    }

    return columns;
  }
}

class DeltaGraphColumnsBuilder {
  twinGraph: TwinGraph;
  ctx: ColumnsCtx;

  constructor(
    twinGraph: TwinGraph,
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
  ) {
    this.twinGraph = twinGraph;
    this.ctx = new ColumnsCtx(graphSettings, setGraphSettings, tvc);
  }

  makeColumns(): Column[] {
    const r = this.twinGraph.rightGraphX();
    const columns: Column[] = [
      new NodeTierColumn(this.ctx, this.twinGraph),
      new TransitiveCountRightInDeltaViewColumn(this.ctx, this.twinGraph),
      new TransitiveCountDeltaColumn(this.ctx, this.twinGraph),
    ];
    for (const metric of r.metricNames) {
      columns.push(
        new MetricRightInDeltaViewColumn(this.ctx, this.twinGraph, metric),
      );
      columns.push(new MetricDeltaViewColumn(this.ctx, this.twinGraph, metric));

      for (const tier of r.stats().tier_names) {
        columns.push(
          new TransitiveTieredMetricRightDeltaColumn(
            this.ctx,
            this.twinGraph,
            metric,
            tier,
          ),
        );
        columns.push(
          new TransitiveTieredMetricDeltaColumn(
            this.ctx,
            this.twinGraph,
            metric,
            tier,
          ),
        );
      }
    }
    return columns;
  }
}

class ConjointCountColumn implements Column {
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
    return this.ctx.showConjointCount && this.ctx.showConjoint;
  }

  getID(): string {
    if (this.side == null) {
      return "C(count)";
    }
    return `C(count)${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          ConjointCount: {
            t: columnType,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "ConjointCount" in sort.column &&
      sort.column.ConjointCount.t === columnType
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

class ParentsCountColumn implements Column {
  ctx: ColumnsCtx;
  nativeGraph: NativeGraph;
  side: GraphSide | null;

  constructor(ctx: ColumnsCtx, nativeGraph: NativeGraph, side?: GraphSide) {
    this.ctx = ctx;
    this.nativeGraph = nativeGraph;
    this.side = side ?? null;
  }

  isEnabled() {
    return this.ctx.showParentsCount;
  }

  getID(): string {
    if (this.side == null) {
      return "Parents #";
    }
    return `Parents # ${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          ParentsCount: {
            t: columnType,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "ParentsCount" in sort.column &&
      sort.column.ParentsCount.t === columnType
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

class ConjointMetricColumn implements Column {
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
      this.ctx.showConjoint &&
      this.ctx.metricSettings(this.metricName)?.show_conjoint_self ===
        "WhenEnabledGlobally"
    );
  }

  getID(): string {
    const base = `C(${this.metricName})`;
    if (this.side == null) {
      return base;
    }
    return `${base} ${this.side === GRAPH_SIDE.L ? "L" : "R"}`;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          ConjointMetric: {
            t: columnType,
            name: this.metricName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "ConjointMetric" in sort.column &&
      sort.column.ConjointMetric.t === columnType &&
      sort.column.ConjointMetric.name === this.metricName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const values =
      this.nativeGraph.getConjointCost().metrics?.[this.metricName] ?? null;
    const getValues = (idxs: NodeIDX[]) =>
      idxs.map((idx) => values?.[idx] ?? 0);
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
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(getValues(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

class ConjointTieredMetricColumn implements Column {
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

    if (metricSettings?.show_conjoint_tiered?.[this.tierName] === "Never") {
      return false;
    }
    return this.ctx.graphSettings.ui_settings?.columns?.show_conjoint === true;
  }

  getID(): string {
    const graphHasMoreThanOneMetric = this.nativeGraph.metricNames.length > 1;

    const base = graphHasMoreThanOneMetric
      ? `${this.tierName} ${this.metricName}`
      : this.tierName;

    return `C(${base})`;
  }

  sortable() {
    const columnType: ColumnType =
      this.side === GRAPH_SIDE.R ? "Right" : "Left";
    const sortable: TSortable = {
      order: null,
      onSortChange: (order: SortOrder | null) =>
        this.ctx.onSortChange(order, {
          ConjointTieredMetric: {
            t: columnType,
            name: this.metricName,
            tier: this.tierName,
          },
        }),
    };

    const sort = this.ctx.sort();
    if (
      sort != null &&
      "ConjointTieredMetric" in sort.column &&
      sort.column.ConjointTieredMetric.t === columnType &&
      sort.column.ConjointTieredMetric.name === this.metricName &&
      sort.column.ConjointTieredMetric.tier === this.tierName
    ) {
      sortable.order = sort.order;
    }

    return sortable;
  }

  definition(): [string, NumericValueColumnDefinition] {
    const columnID = this.getID();
    const values =
      this.nativeGraph.getConjointCost().tiered_metric?.[this.metricName]?.[
        this.tierName
      ];
    const getValues = (idxs: NodeIDX[]) =>
      idxs.map((idx) => values?.[idx] ?? 0);
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
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(getValues(idxs));
      },
      sortable: this.sortable(),
      isHidden: false,
    };

    return [columnID, definition];
  }
}

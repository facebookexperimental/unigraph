import clsx from "clsx";
import { useMemo } from "react";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../ArrowUtils";
import { H1, H2, Link, Pre } from "../Typography";
import type { ColumnType } from "../__generated__/ts/ColumnType";
import type { GraphSettings } from "../__generated__/ts/GraphSettings";
import type { GraphTableSort } from "../__generated__/ts/GraphTableSort";
import type { MetricFormat } from "../__generated__/ts/MetricFormat";
import type { MetricSettings } from "../__generated__/ts/MetricSettings";
import type { SortColumn } from "../__generated__/ts/SortColumn";
import type { SortOrder } from "../__generated__/ts/SortOrder";
import type { TraversalConfig } from "../__generated__/ts/TraversalConfig";
import UTooltip from "../components/UTooltip";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useTwinGraph } from "../context/NativeGraphContext";
import { useTVC } from "../context/TraversalConfigContext";
import ConjointCostDocs from "../inline_docs/ConjointCost";
import formatMetric from "../lib/formatMetric";
import type NativeGraph from "../native/NativeGraph";
import { GRAPH_SIDE, type GraphSide } from "../native/NativeGraph";
import type { NodeIDX } from "../types";
import ContextMenuCell from "./ContextMenuCell";
import type {
  ColumnDefinitions,
  ColumnID,
  NonTreeColumnDefinition,
  NumericValueColumnDefinition,
  TSortable,
  TreeColumnDefinition,
} from "./TreeTable";
import type { Row } from "./TreeTableRows";

const NO_PRECISION_FORMAT: MetricFormat = {
  NumberWithVariablePrecision: {
    min_precision: 0,
    max_precision: 0,
    use_delimiter: true,
  },
};

interface Column {
  isEnabled: () => boolean;
  definition: () => [string, NonTreeColumnDefinition];
  sortable: () => TSortable | null;
}

export default function useGraphTreeTableColumns(): ColumnDefinitions {
  const twinGraph = useTwinGraph();
  const l = twinGraph.l;
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const { tvc } = useTVC();

  return useMemo(() => {
    const builder =
      twinGraph.r !== null
        ? new DeltaGraphColumnsBuilder()
        : new SingleGraphColumnsBuilder(
            l,
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

    nonTreeColumns.context_menu = {
      t: "non_sortable_column",
      label: "More Menu",
      renderer: (row: Readonly<Row>) => <ContextMenuCell row={row} />,
      isHidden: false,
      isLabelHidden: true,
    };

    return {
      treeColumn,
      columns: nonTreeColumns,
    };
  }, [twinGraph, l, graphSettings, setGraphSettings, tvc]);
}

function MetricCell({
  value,
  format,
}: { value: number; format?: MetricFormat }) {
  return (
    <p className="px-4 text-right tabular-nums w-full whitespace-nowrap">
      {formatMetric(value, format)}
    </p>
  );
}

function WouldBeDeltaMetricCell({
  value,
  format,
}: { value: number; format?: MetricFormat }) {
  let sign = "";
  if (value < 0) {
    sign = "-";
  } else {
    sign = "+";
  }

  return (
    <UTooltip
      tooltip={
        "This node is not reachable in the current graph. This value represents how much the value for the whole graph would change if this edge was included."
      }
    >
      <p
        className={clsx(
          "px-2 mx-2 py-1 text-right tabular-nums w-full whitespace-nowrap",
          {
            "text-green-500": value < 0,
            "text-red-500": value >= 0,
          },
        )}
      >
        {sign}
        {formatMetric(value, format)}
      </p>
    </UTooltip>
  );
}

function MissingMetric() {
  return (
    <p className="px-4 text-right tabular-nums w-full whitespace-nowrap">-</p>
  );
}

/// Simple context type to capture the current settings on the graph
/// with consistent defaults.
class ColumnsCtx {
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
  nativeGraph: NativeGraph;
  graphSettings: GraphSettings;
  setGraphSettings: (gs: GraphSettings) => void;
  tvc: TraversalConfig;
  ctx: ColumnsCtx;
  columns: Column[] = [];

  constructor(
    nativeGraph: NativeGraph,
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
  ) {
    this.nativeGraph = nativeGraph;
    this.graphSettings = graphSettings;
    this.setGraphSettings = setGraphSettings;
    this.tvc = tvc;
    this.ctx = new ColumnsCtx(graphSettings, setGraphSettings, tvc);
    this.columns = [];
  }

  makeColumns(): Column[] {
    const { ctx, nativeGraph: g } = this;
    const columns: Column[] = [
      new TransitiveCountColumn(ctx, g),
      new ConjointCountColumn(ctx, g),
      new ParentsCountColumn(ctx, g),
    ];

    for (const metric of this.nativeGraph.metricNames) {
      columns.push(new MetricColumn(ctx, g, metric));
      columns.push(new TransitiveMetricColumn(ctx, g, metric));
      columns.push(new ConjointMetricColumn(ctx, g, metric));

      for (const tier of this.nativeGraph.stats().tier_names) {
        columns.push(new TransitiveTieredMetricColumn(ctx, g, metric, tier));
        columns.push(new ConjointTieredMetricColumn(ctx, g, metric, tier));
      }
    }

    return columns;
  }
}

class DeltaGraphColumnsBuilder {
  makeColumns(): Column[] {
    return [];
  }
}

class TransitiveCountColumn implements Column {
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

class MetricColumn implements Column {
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

class TransitiveMetricColumn implements Column {
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

class TransitiveTieredMetricColumn implements Column {
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
    const tierIDX = this.nativeGraph
      .stats()
      .tier_names.findIndex((name) => name === this.tierName);

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
    const tierIDX = this.nativeGraph
      .stats()
      .tier_names.findIndex((name) => name === this.tierName);

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

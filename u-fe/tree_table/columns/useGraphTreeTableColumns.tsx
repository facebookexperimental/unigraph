// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useMemo } from "react";
import type { GraphSettings } from "../../__generated__/ts/GraphSettings";
import type { GraphStructure } from "../../__generated__/ts/GraphStructure";
import type { GraphTableSort } from "../../__generated__/ts/GraphTableSort";
import type { SortColumn } from "../../__generated__/ts/SortColumn";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import type { TraversalConfig } from "../../__generated__/ts/TraversalConfig";
import { displayNodeName } from "../../lib/utils";
import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import { useGraphSettings } from "../../context/GraphSettingsContext";
import { useMetricViewState } from "../../context/MetricViewStateContext";
import { useTwinGraph } from "../../context/NativeGraphContext";
import { useTVC } from "../../context/TraversalConfigContext";
import type TwinGraph from "../../native/TwinGraph";
import type { NodeIDX } from "../../types";
import ContextMenuCell from "../ContextMenuCell";
import type {
  ColumnDefinitions,
  ColumnID,
  NonTreeColumnDefinition,
  TreeColumnDefinition,
  TSortable,
} from "../columns";
import type { Row } from "../TreeTableRows";
import {
  DominatedCountColumn,
  ParentsCountColumn,
  TransitiveCountColumn,
  TransitiveCountDeltaColumn,
  TransitiveCountRightInDeltaViewColumn,
} from "./counts";
import {
  DominatedMetricColumn,
  EnumMetricColumn,
  MetricColumn,
  MetricDeltaViewColumn,
  MetricRightInDeltaViewColumn,
  TieredDominatedMetricColumn,
  TransitiveMetricColumn,
  TransitiveMetricDeltaColumn,
  TransitiveMetricRightInDeltaViewColumn,
  TransitiveTieredMetricColumn,
  TransitiveTieredMetricDeltaColumn,
  TransitiveTieredMetricRightDeltaColumn,
} from "./metrics";
import { NodeTierColumn } from "./tiers";

export interface Column {
  isEnabled: () => boolean;
  definition: () => [string, NonTreeColumnDefinition];
  sortable: () => TSortable | null;
}

export default function useGraphTreeTableColumns(): ColumnDefinitions {
  const twinGraph = useTwinGraph();
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const { visibleViews } = useMetricViewState();
  const { tvcR: tvc } = useTVC();

  return useMemo(() => {
    const builder =
      twinGraph.l !== null
        ? new DeltaGraphColumnsBuilder(
            twinGraph,
            graphSettings,
            setGraphSettings,
            tvc,
            visibleViews,
          )
        : new SingleGraphColumnsBuilder(
            twinGraph,
            graphSettings,
            setGraphSettings,
            tvc,
            visibleViews,
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
      getNodeName: (idx: NodeIDX) =>
        displayNodeName(twinGraph.r.getNodeName(idx)),
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

    if (twinGraph.l === null) {
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
  }, [twinGraph, graphSettings, setGraphSettings, tvc, visibleViews]);
}

export class ColumnsCtx {
  graphSettings: GraphSettings;
  setGraphSettings: (gs: GraphSettings) => void;
  tvc: TraversalConfig;
  visibleViews: Set<string>;
  showMetrics: boolean;
  showCounts: boolean;
  graphStructure: GraphStructure;

  constructor(
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
    visibleViews: Set<string>,
  ) {
    this.graphSettings = graphSettings;
    this.setGraphSettings = setGraphSettings;
    this.tvc = tvc;
    this.visibleViews = visibleViews;

    this.showMetrics =
      graphSettings.ui_settings?.columns?.hide_metrics !== true;
    this.showCounts = graphSettings.ui_settings?.columns?.show_counts === true;
    this.graphStructure =
      graphSettings.ui_settings?.graph_structure ?? "Forward";
  }

  isVisible(viewKey: string): boolean {
    return this.visibleViews.has(viewKey);
  }

  format(metricName: string): MetricFormat | undefined {
    return this.graphSettings?.metrics_config?.metrics?.[metricName]?.format;
  }

  /// An "enum metric" is a numeric metric whose value maps to a categorical
  /// label. Its transitive/dominated/tiered aggregations (which sum values
  /// over descendants) are meaningless, so it collapses to a single column.
  isEnum(metricName: string): boolean {
    const fmt = this.format(metricName);
    return fmt != null && "Enum" in fmt;
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

export class SingleGraphColumnsBuilder {
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
    visibleViews: Set<string>,
  ) {
    this.twinGraph = twinGraph;
    this.graphSettings = graphSettings;
    this.setGraphSettings = setGraphSettings;
    this.tvc = tvc;
    this.ctx = new ColumnsCtx(
      graphSettings,
      setGraphSettings,
      tvc,
      visibleViews,
    );
    this.columns = [];
  }

  makeColumns(): Column[] {
    const { ctx, twinGraph } = this;
    const g = twinGraph.r;
    const columns: Column[] = [
      new NodeTierColumn(this.ctx, this.twinGraph),
      new TransitiveCountColumn(ctx, g),
      new DominatedCountColumn(ctx, g),
      new ParentsCountColumn(ctx, g),
    ];

    for (const metric of g.metricNames) {
      if (ctx.isEnum(metric)) {
        columns.push(new EnumMetricColumn(ctx, this.twinGraph, metric));
        continue;
      }

      columns.push(new MetricColumn(ctx, g, metric));
      columns.push(new TransitiveMetricColumn(ctx, g, metric));
      columns.push(new DominatedMetricColumn(ctx, g, metric));

      for (const tier of g.stats().tier_names) {
        columns.push(new TransitiveTieredMetricColumn(ctx, g, metric, tier));
        columns.push(new TieredDominatedMetricColumn(ctx, g, metric, tier));
      }
    }

    return columns;
  }
}

export class DeltaGraphColumnsBuilder {
  twinGraph: TwinGraph;
  ctx: ColumnsCtx;

  constructor(
    twinGraph: TwinGraph,
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
    visibleViews: Set<string>,
  ) {
    this.twinGraph = twinGraph;
    this.ctx = new ColumnsCtx(
      graphSettings,
      setGraphSettings,
      tvc,
      visibleViews,
    );
  }

  makeColumns(): Column[] {
    const g = this.twinGraph.r;
    const columns: Column[] = [
      new NodeTierColumn(this.ctx, this.twinGraph),
      new TransitiveCountRightInDeltaViewColumn(this.ctx, this.twinGraph),
      new TransitiveCountDeltaColumn(this.ctx, this.twinGraph),
    ];

    for (const metric of g.metricNames) {
      if (this.ctx.isEnum(metric)) {
        columns.push(new EnumMetricColumn(this.ctx, this.twinGraph, metric));
        continue;
      }

      columns.push(
        new MetricRightInDeltaViewColumn(this.ctx, this.twinGraph, metric),
      );
      columns.push(new MetricDeltaViewColumn(this.ctx, this.twinGraph, metric));
      columns.push(
        new TransitiveMetricRightInDeltaViewColumn(
          this.ctx,
          this.twinGraph,
          metric,
        ),
      );
      columns.push(
        new TransitiveMetricDeltaColumn(this.ctx, this.twinGraph, metric),
      );

      for (const tier of g.stats().tier_names) {
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

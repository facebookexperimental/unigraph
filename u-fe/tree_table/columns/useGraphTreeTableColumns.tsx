// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useMemo } from "react";
import type { GraphSettings } from "../../__generated__/ts/GraphSettings";
import type { GraphStructure } from "../../__generated__/ts/GraphStructure";
import type { GraphTableSort } from "../../__generated__/ts/GraphTableSort";
import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import type { MetricsConfig } from "../../__generated__/ts/MetricsConfig";
import type { MetricViewVisibility } from "../../__generated__/ts/MetricViewVisibility";
import type { SortColumn } from "../../__generated__/ts/SortColumn";
import type { SortOrder } from "../../__generated__/ts/SortOrder";
import type { TraversalConfig } from "../../__generated__/ts/TraversalConfig";
import { displayNodeName } from "../../lib/utils";
import { useGraphSettings } from "../../context/GraphSettingsContext";
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
  isMetricAvailable,
  isStructuralAvailable,
  metricFormatFromConfig,
} from "./ColumnUtils";
import {
  DominatedCountColumn,
  ParentsCountColumn,
  TransitiveCountColumn,
  TransitiveCountDeltaColumn,
  TransitiveCountRightInDeltaViewColumn,
} from "./counts";
import {
  DominatedMetricColumn,
  MetricColumn,
  MetricDeltaViewColumn,
  MetricRightInDeltaViewColumn,
  TieredDominatedMetricColumn,
  TransitiveMetricColumn,
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
  const { tvcR: tvc } = useTVC();

  return useMemo(() => {
    const builder =
      twinGraph.l !== null
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
  }, [twinGraph, graphSettings, setGraphSettings, tvc]);
}

/// Simple context type to capture the current settings on the graph
/// with consistent defaults.
export class ColumnsCtx {
  graphSettings: GraphSettings;
  setGraphSettings: (gs: GraphSettings) => void;
  tvc: TraversalConfig;
  metricsConfig: MetricsConfig | undefined;
  showMetrics: boolean;
  showTieredMetrics: boolean;
  hideDominatedTieredMetrics: boolean;
  showCounts: boolean;
  graphStructure: GraphStructure;

  constructor(
    graphSettings: GraphSettings,
    setGraphSettings: (gs: GraphSettings) => void,
    tvc: TraversalConfig,
  ) {
    this.graphSettings = graphSettings;
    this.setGraphSettings = setGraphSettings;
    this.tvc = tvc;
    this.metricsConfig = graphSettings.metrics_config;

    this.showMetrics =
      graphSettings.ui_settings?.columns?.hide_metrics !== true;
    this.showTieredMetrics =
      graphSettings.ui_settings?.columns?.show_tiered_metrics === true;
    this.hideDominatedTieredMetrics =
      graphSettings.ui_settings?.columns?.hide_dominated_tiered_metrics ===
      true;
    this.showCounts = graphSettings.ui_settings?.columns?.show_counts === true;
    this.graphStructure =
      graphSettings.ui_settings?.graph_structure ?? "Forward";
  }

  viewVisibility(viewKey: string): MetricViewVisibility | undefined {
    return this.graphSettings.metrics_visibility?.[viewKey];
  }

  resolvedVisibility(
    viewKey: string,
    viewType:
      | "self_view"
      | "transitive"
      | "dominated"
      | "tiered"
      | "tiered_dominated",
  ): MetricViewVisibility {
    const explicit = this.graphSettings.metrics_visibility?.[viewKey];
    if (explicit != null) return explicit;
    const dv = this.metricsConfig?.default_visibility;
    if (dv != null) {
      const perType = dv[viewType];
      if (perType != null) return perType;
      if (dv.all != null) return dv.all;
    }
    if (viewType === "dominated" || viewType === "tiered_dominated") {
      return "EnabledInDominatorMode";
    }
    return "Enabled";
  }

  metricFormat(metricName: string): MetricFormat | undefined {
    return metricFormatFromConfig(this.metricsConfig, metricName);
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
    const g = twinGraph.r;
    const mc = ctx.metricsConfig;
    const columns: Column[] = [new NodeTierColumn(this.ctx, this.twinGraph)];

    if (isStructuralAvailable(mc, "count_transitive")) {
      columns.push(new TransitiveCountColumn(ctx, g));
    }
    if (isStructuralAvailable(mc, "count_dominated")) {
      columns.push(new DominatedCountColumn(ctx, g));
    }
    if (isStructuralAvailable(mc, "parents_count")) {
      columns.push(new ParentsCountColumn(ctx, g));
    }

    for (const metric of g.metricNames) {
      if (isMetricAvailable(mc, metric, "self_view")) {
        columns.push(new MetricColumn(ctx, g, metric));
      }
      if (isMetricAvailable(mc, metric, "transitive")) {
        columns.push(new TransitiveMetricColumn(ctx, g, metric));
      }
      if (isMetricAvailable(mc, metric, "dominated")) {
        columns.push(new DominatedMetricColumn(ctx, g, metric));
      }

      for (const tier of g.stats().tier_names) {
        if (isMetricAvailable(mc, metric, "tiered")) {
          columns.push(new TransitiveTieredMetricColumn(ctx, g, metric, tier));
        }
        if (isMetricAvailable(mc, metric, "tiered_dominated")) {
          columns.push(new TieredDominatedMetricColumn(ctx, g, metric, tier));
        }
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
    const r = this.twinGraph.r;
    const mc = this.ctx.metricsConfig;
    const columns: Column[] = [new NodeTierColumn(this.ctx, this.twinGraph)];

    if (isStructuralAvailable(mc, "count_transitive")) {
      columns.push(
        new TransitiveCountRightInDeltaViewColumn(this.ctx, this.twinGraph),
      );
      columns.push(new TransitiveCountDeltaColumn(this.ctx, this.twinGraph));
    }

    for (const metric of r.metricNames) {
      if (isMetricAvailable(mc, metric, "self_view")) {
        columns.push(
          new MetricRightInDeltaViewColumn(this.ctx, this.twinGraph, metric),
        );
        columns.push(
          new MetricDeltaViewColumn(this.ctx, this.twinGraph, metric),
        );
      }

      for (const tier of r.stats().tier_names) {
        if (isMetricAvailable(mc, metric, "tiered")) {
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
    }
    return columns;
  }
}

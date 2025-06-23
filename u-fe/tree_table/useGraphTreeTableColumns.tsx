import type { NodeIDX } from "@/types";
import { useMemo } from "react";
import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import type { GraphSettings } from "u-be/unigraph_core/bindings/GraphSettings";
import type { MetricSettings } from "u-be/unigraph_core/bindings/MetricSettings";
import type NativeGraph from "../NativeGraph";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useNativeGraph } from "../context/NativeGraphContext";
import formatMetric from "../lib/formatMetric";
import formatNumber from "../lib/formatNumber";
import ContextMenuCell from "./ContextMenuCell";
import type {
  ColumnDefinitions,
  NonTreeColumnDefinition,
  NumericValueColumnDefinition,
  TreeColumnDefinition,
} from "./TreeTable";

export default function useGraphTreeTableColumns(): ColumnDefinitions {
  const nativeGraph = useNativeGraph();
  const [graphSettings] = useGraphSettings();

  return useMemo(() => {
    return defaultColumnDefinitions(nativeGraph, graphSettings);
  }, [nativeGraph, graphSettings]);
}

function defaultColumnDefinitions(
  nativeGraph: NativeGraph,
  graphSettings: GraphSettings,
): ColumnDefinitions {
  const columnDefinitions: { [name: string]: NonTreeColumnDefinition } = {};
  for (const metricName of nativeGraph.metricNames) {
    const metricSettings = graphSettings.metric_settings?.[metricName] ?? null;

    if (metricSettings?.column_hide_self !== true) {
      const [metricColumnID, metricColumnDefinition] = createMetricColumn(
        nativeGraph,
        metricName,
        metricSettings,
      );

      columnDefinitions[metricColumnID] = metricColumnDefinition;
    }

    if (metricSettings?.column_hide_transitive !== true) {
      const [transitiveMetricColumnID, transitiveMetricColumnDefinition] =
        createTransitiveMetricColumn(metricName, nativeGraph, metricSettings);

      columnDefinitions[transitiveMetricColumnID] =
        transitiveMetricColumnDefinition;
    }

    const tieredTransitiveColumns = createTieredTransitiveMetricColumn(
      metricName,
      nativeGraph,
      metricSettings,
    );

    for (const { columnID, definition } of tieredTransitiveColumns) {
      columnDefinitions[columnID] = definition;
    }

    if (graphSettings.ui_settings?.columns?.show_parents_count === true) {
      const [parentsCountColumnID, parentsCountColumnDefinition] =
        createParentsCountColumn(nativeGraph);
      columnDefinitions[parentsCountColumnID] = parentsCountColumnDefinition;
    }

    if (graphSettings.ui_settings?.columns?.show_transitive_count === true) {
      const [transitiveCountColumnID, transitiveCountColumnDefinition] =
        createTransitiveCountColumn(nativeGraph);
      columnDefinitions[transitiveCountColumnID] =
        transitiveCountColumnDefinition;

      const [
        transitiveCountDominatedColumnID,
        transitiveCountDominatedColumnDefinition,
      ] = createTransitiveCountDominatedColumn(nativeGraph);
      columnDefinitions[transitiveCountDominatedColumnID] =
        transitiveCountDominatedColumnDefinition;
    }
  }

  const treeColumn: TreeColumnDefinition = {
    label: "Node Name",
    getNodeName: (idx: NodeIDX) => nativeGraph.getNodeName(idx),
    flexGrow: 1,
  };

  columnDefinitions.context_menu = {
    t: "non_sortable_column",
    label: "More Menu",
    renderer: (arrow: Arrow) => <ContextMenuCell arrow={arrow} />,
    isHidden: false,
    isLabelHidden: true,
  };

  return {
    treeColumn,
    columns: columnDefinitions,
  };
}

function createMetricColumn(
  nativeGraph: NativeGraph,
  metricName: string,
  metricSettings: MetricSettings | null,
): [string, NumericValueColumnDefinition] {
  const columnID = `[metric] ${metricName}`;
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: metricName,
    renderer: (arrow: Arrow) => {
      const value = !nativeGraph.isNodeReachable(arrow.points_to)
        ? "-"
        : formatMetric(
            nativeGraph.getNodeMetric(arrow.points_to, metricName),
            metricSettings?.format,
          );
      return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
    },
    getNumericValues: (idxs: NodeIDX[]) =>
      nativeGraph.getNodeMetricBatched(idxs, metricName),
    sortable: true,
    isHidden: false,
  };

  return [columnID, definition];
}

function createTransitiveMetricColumn(
  metricName: string,
  nativeGraph: NativeGraph,
  metricSettings: MetricSettings | null,
): [string, NumericValueColumnDefinition] {
  const columnID = `[transitive] ${metricName}`;
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: columnID,
    renderer: (arrow: Arrow) => {
      const value = !nativeGraph.isNodeReachable(arrow.points_to)
        ? "-"
        : formatMetric(
            nativeGraph.getTransitiveMetric(arrow.points_to, metricName),
            metricSettings?.format,
          );
      return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      return nativeGraph.getTransitiveMetricsBatched(idxs, metricName);
    },
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function createTieredTransitiveMetricColumn(
  metricName: string,
  nativeGraph: NativeGraph,
  metricSettings: MetricSettings | null,
): { columnID: string; definition: NumericValueColumnDefinition }[] {
  const tiers = nativeGraph.stats().tier_names;
  const column_hide_trantitive_tiered =
    metricSettings?.column_hide_trantitive_tiered ?? [];

  return tiers.flatMap((tierName) => {
    if (column_hide_trantitive_tiered.includes(tierName)) {
      return [];
    }

    const columnID = `[tiered_transitive ${tierName}] ${metricName}`;
    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: tierName,
      renderer: (arrow: Arrow) => {
        const value = !nativeGraph.isNodeReachable(arrow.points_to)
          ? "-"
          : formatMetric(
              nativeGraph.getTieredTransitiveMetric(
                arrow.points_to,
                metricName,
              )?.[tierName] ?? 0,
              metricSettings?.format,
            );
        return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(
          nativeGraph
            .getTieredTransitiveMetricsBatched(idxs, metricName)
            .map((m) => m[tierName] ?? 0),
        );
      },
      sortable: true,
      isHidden: false,
    };

    return [
      {
        columnID,
        definition,
      },
    ];
  });
}

function createParentsCountColumn(
  nativeGraph: NativeGraph,
): [string, NumericValueColumnDefinition] {
  const columnID = "Parents #";
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: columnID,
    renderer: (arrow: Arrow) => {
      const value = formatNumber(
        nativeGraph.getParentsCount([arrow.points_to])[0] ?? 0,
        0,
        0,
        true,
      );
      return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      return nativeGraph.getParentsCount(idxs);
    },
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function createTransitiveCountColumn(
  nativeGraph: NativeGraph,
): [string, NumericValueColumnDefinition] {
  const columnID = "T(count)";
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: columnID,
    renderer: (arrow: Arrow) => {
      const value = formatNumber(
        nativeGraph.getTransitiveCount([arrow.points_to])[0] ?? 0,
        0,
        0,
        true,
      );
      return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      return nativeGraph.getTransitiveCount(idxs);
    },
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function createTransitiveCountDominatedColumn(
  nativeGraph: NativeGraph,
): [string, NumericValueColumnDefinition] {
  const columnID = "D(T(count))";
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: columnID,
    renderer: (arrow: Arrow) => {
      const value = formatNumber(
        nativeGraph.getTransitiveCountDominated([arrow.points_to])[0] ?? 0,
        0,
        0,
        true,
      );
      return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      return nativeGraph.getTransitiveCountDominated(idxs);
    },
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

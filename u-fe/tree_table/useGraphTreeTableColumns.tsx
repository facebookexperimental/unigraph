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
  const showTransitive =
    graphSettings.ui_settings?.columns?.show_transitive ?? false;
  const showConjoint =
    graphSettings.ui_settings?.columns?.show_conjoint ?? false;
  const showTiered = graphSettings.ui_settings?.columns?.hide_tiered !== true;

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

    if (showTransitive && metricSettings?.column_show_transitive !== "Never") {
      const [transitiveMetricColumnID, transitiveMetricColumnDefinition] =
        createTransitiveMetricColumn(metricName, nativeGraph, metricSettings);

      columnDefinitions[transitiveMetricColumnID] =
        transitiveMetricColumnDefinition;
    }

    if (showTiered) {
      const tieredTransitiveColumns = createTieredTransitiveMetricColumn(
        metricName,
        nativeGraph,
        metricSettings,
        graphSettings.ui_settings?.graph_structure === "Dominator",
      );
      for (const { columnID, definition } of tieredTransitiveColumns) {
        columnDefinitions[columnID] = definition;
      }
    }

    if (graphSettings.ui_settings?.columns?.show_parents_count === true) {
      const [parentsCountColumnID, parentsCountColumnDefinition] =
        createParentsCountColumn(nativeGraph);
      columnDefinitions[parentsCountColumnID] = parentsCountColumnDefinition;
    }

    if (
      showTransitive &&
      graphSettings.ui_settings?.columns?.show_transitive_count !== "Never"
    ) {
      const [transitiveCountColumnID, transitiveCountColumnDefinition] =
        createTransitiveCountColumn(nativeGraph);
      columnDefinitions[transitiveCountColumnID] =
        transitiveCountColumnDefinition;

      if (graphSettings.ui_settings?.graph_structure === "Dominator") {
        const [
          transitiveCountDominatedColumnID,
          transitiveCountDominatedColumnDefinition,
        ] = createTransitiveCountDominatedColumn(nativeGraph);
        columnDefinitions[transitiveCountDominatedColumnID] =
          transitiveCountDominatedColumnDefinition;
      }
    }

    if (showConjoint) {
      if (graphSettings.ui_settings?.columns?.show_conjoint_count !== "Never") {
        const [conjointCountColumnID, conjointCountColumnDefinition] =
          createConjointCountColumn(nativeGraph);
        columnDefinitions[conjointCountColumnID] =
          conjointCountColumnDefinition;
      }

      const metricsConjointColumns = createMetricsConjointMetricColumn(
        metricName,
        nativeGraph,
        metricSettings,
      );

      for (const { columnID, definition } of metricsConjointColumns) {
        columnDefinitions[columnID] = definition;
      }
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
      return <MetricCell value={value} />;
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
      return <MetricCell value={value} />;
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
  dominated: boolean,
): { columnID: string; definition: NumericValueColumnDefinition }[] {
  const tiers = nativeGraph.stats().tier_names;

  return tiers.flatMap((tierName) => {
    if (metricSettings?.column_show_tiered?.[tierName] === "Never") {
      return [];
    }

    const [columnID, label, getValue] = (() => {
      if (dominated) {
        const columnID = `[tiered_transitive dominated ${tierName}] ${metricName}`;
        const label = `D(${tierName})`;
        const getValue = (idxs: NodeIDX[]) =>
          nativeGraph
            .getTieredTransitiveMetricsDominatedBatched(idxs, metricName)
            .map((m) => m[tierName] ?? 0);

        return [columnID, label, getValue];
      } else {
        const columnID = `[tiered_transitive ${tierName}] ${metricName}`;
        const label = tierName;
        const getValue = (idxs: NodeIDX[]) =>
          nativeGraph
            .getTieredTransitiveMetricsBatched(idxs, metricName)
            .map((m) => m[tierName] ?? 0);

        return [columnID, label, getValue];
      }
    })();

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label,
      renderer: (arrow: Arrow) => {
        const value = !nativeGraph.isNodeReachable(arrow.points_to)
          ? "-"
          : formatMetric(
              getValue([arrow.points_to])[0] as number,
              metricSettings?.format,
            );
        return <MetricCell value={value} />;
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(getValue(idxs));
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

function createMetricsConjointMetricColumn(
  metricName: string,
  nativeGraph: NativeGraph,
  metricSettings: MetricSettings | null,
): { columnID: string; definition: NumericValueColumnDefinition }[] {
  const tiers = nativeGraph.stats().tier_names;

  const metricConjColumn = (() => {
    if (metricSettings?.show_conjoint_self === "Never") {
      return [];
    }

    const columnID = `[conjoint ${metricName}`;
    const label = `C(${metricName})`;
    const values = nativeGraph.getConjointCost().metrics?.[metricName] ?? null;
    const getValues = (idxs: NodeIDX[]) =>
      idxs.map((idx) => values?.[idx] ?? 0);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label,
      renderer: (arrow: Arrow) => {
        const value = !nativeGraph.isNodeReachable(arrow.points_to)
          ? "-"
          : formatMetric(
              getValues([arrow.points_to])[0] as number,
              metricSettings?.format,
            );
        return <MetricCell value={value} />;
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(getValues(idxs));
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
  })();

  const tieredColumns = tiers.flatMap((tierName) => {
    if (metricSettings?.show_conjoint_tiered?.[tierName] === "Never") {
      return [];
    }

    const columnID = `[tiered_conjoint ${tierName}] ${metricName}`;
    const label = `C(${tierName})`;
    const values =
      nativeGraph.getConjointCost().tiered_metric?.[metricName]?.[tierName];
    const getValues = (idxs: NodeIDX[]) =>
      idxs.map((idx) => values?.[idx] ?? 0);

    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label,
      renderer: (arrow: Arrow) => {
        const value = !nativeGraph.isNodeReachable(arrow.points_to)
          ? "-"
          : formatMetric(
              getValues([arrow.points_to])[0] as number,
              metricSettings?.format,
            );
        return <MetricCell value={value} />;
      },
      getNumericValues: (idxs: NodeIDX[]) => {
        return new Float32Array(getValues(idxs));
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

  return [...metricConjColumn, ...tieredColumns];
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
      return <MetricCell value={value} />;
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
      return <MetricCell value={value} />;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      return nativeGraph.getTransitiveCount(idxs);
    },
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function createConjointCountColumn(
  nativeGraph: NativeGraph,
): [string, NumericValueColumnDefinition] {
  const columnID = "C(count)";
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: columnID,
    renderer: (arrow: Arrow) => {
      const value = formatNumber(
        nativeGraph.getConjointCost().count[arrow.points_to] ?? 0,
        0,
        0,
        true,
      );
      return <MetricCell value={value} />;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      const count = nativeGraph.getConjointCost().count;
      return new Float32Array(idxs.map((idx) => count[idx] ?? 0));
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
      return <MetricCell value={value} />;
    },
    getNumericValues: (idxs: NodeIDX[]) => {
      return nativeGraph.getTransitiveCountDominated(idxs);
    },
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function MetricCell({ value }: { value: string }) {
  return (
    <p className="px-4 text-right tabular-nums w-full whitespace-nowrap">
      {value}
    </p>
  );
}

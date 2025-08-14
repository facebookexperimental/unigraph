import type { GraphSettings } from "@/__generated__/ts/GraphSettings";
import type { MetricFormat } from "@/__generated__/ts/MetricFormat";
import type { MetricSettings } from "@/__generated__/ts/MetricSettings";
import type { TraversalConfig } from "@/__generated__/ts/TraversalConfig";
import type { NodeIDX } from "@/types";
import clsx from "clsx";
import { useMemo } from "react";
import { ARROW_POINTS_FROM_NON_EXISTENT } from "../ArrowUtils";
import type NativeGraph from "../NativeGraph";
import { GRAPH_SIDE, type GraphSide } from "../NativeGraph";
import type TwinGraph from "../TwinGraph";
import UTooltip from "../components/UTooltip";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useTwinGraph } from "../context/NativeGraphContext";
import { useTVC } from "../context/TraversalConfigContext";
import formatMetric from "../lib/formatMetric";
import ContextMenuCell from "./ContextMenuCell";
import type {
  ColumnDefinitions,
  NonTreeColumnDefinition,
  NumericValueColumnDefinition,
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

export default function useGraphTreeTableColumns(): ColumnDefinitions {
  const twinGraph = useTwinGraph();
  const [graphSettings] = useGraphSettings();
  const { tvc } = useTVC();

  return useMemo(() => {
    // TODO: make it work with the right graph
    return defaultColumnDefinitions(twinGraph, graphSettings, tvc);
  }, [twinGraph, graphSettings, tvc]);
}

function defaultColumnDefinitions(
  twinGraph: TwinGraph,
  graphSettings: GraphSettings,
  tvc: TraversalConfig,
): ColumnDefinitions {
  const showTransitive =
    graphSettings.ui_settings?.columns?.show_transitive ?? false;
  const showConjoint =
    graphSettings.ui_settings?.columns?.show_conjoint ?? false;
  const showMetrics = graphSettings.ui_settings?.columns?.hide_metrics !== true;
  const showTiered = graphSettings.ui_settings?.columns?.show_tiered === true;
  const dominated = graphSettings.ui_settings?.graph_structure === "Dominator";
  const showWouldBeMetric = !twinGraph.isDeltaGraph();

  const nativeGraph = twinGraph.l;
  const nativeGraphR = twinGraph.r;

  const columnDefinitions: { [name: string]: NonTreeColumnDefinition } = {};
  for (const metricName of nativeGraph.metricNames) {
    const metricSettings =
      graphSettings.ui_settings?.columns?.metric_settings?.[metricName] ?? null;

    if (metricSettings?.column_hide_self !== true && showMetrics) {
      const [metricColumnID, metricColumnDefinition] = createMetricColumn(
        nativeGraph,
        metricName,
        metricSettings,
      );

      columnDefinitions[metricColumnID] = metricColumnDefinition;
    }

    if (
      showTransitive &&
      metricSettings?.column_show_transitive === "WhenEnabledGlobally"
    ) {
      const [transitiveMetricColumnID, transitiveMetricColumnDefinition] =
        createTransitiveMetricColumn(
          metricName,
          nativeGraph,
          metricSettings,
          dominated,
        );

      columnDefinitions[transitiveMetricColumnID] =
        transitiveMetricColumnDefinition;
    }

    if (showTiered) {
      const tieredTransitiveColumns = createTieredTransitiveMetricColumn(
        metricName,
        nativeGraph,
        metricSettings,
        tvc,
        dominated,
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
      const [transitiveCountColumnIDL, transitiveCountColumnDefinitionL] =
        createTransitiveCountColumn(
          twinGraph.l,
          dominated,
          GRAPH_SIDE.L,
          showWouldBeMetric,
        );

      columnDefinitions[transitiveCountColumnIDL] =
        transitiveCountColumnDefinitionL;

      if (nativeGraphR != null) {
        const [transitiveCountColumnIDR, transitiveCountColumnDefinitionR] =
          createTransitiveCountColumn(
            nativeGraphR,
            dominated,
            GRAPH_SIDE.R,
            showWouldBeMetric,
          );
        columnDefinitions[transitiveCountColumnIDR] =
          transitiveCountColumnDefinitionR;

        const [
          transitiveCountDeltaColumnIDLR,
          transitiveCountDeltaColumnDefinitionLR,
        ] = createTransitiveCountDeltaColumn(
          nativeGraph,
          nativeGraphR,
          dominated,
        );
        columnDefinitions[transitiveCountDeltaColumnIDLR] =
          transitiveCountDeltaColumnDefinitionLR;
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
    renderer: (row: Readonly<Row>) => <ContextMenuCell row={row} />,
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
    renderer: (row: Readonly<Row>) => {
      if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
        return (
          <MetricCell
            value={nativeGraph.getNodeMetric(
              row.arrow_pair.points_to,
              metricName,
            )}
            format={metricSettings?.format}
          />
        );
      } else {
        return <MissingMetric />;
      }
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
  dominated: boolean,
): [string, NumericValueColumnDefinition] {
  const columnID = `transitive_${metricName}`;

  const { getValues, label } = (() => {
    if (dominated) {
      return {
        getValues: (idxs: NodeIDX[]) =>
          nativeGraph.getTransitiveDominatedMetricsBatched(idxs, metricName),
        label: `D(${metricName})`,
      };
    } else {
      return {
        getValues: (idxs: NodeIDX[]) =>
          nativeGraph.getTransitiveMetricsBatched(idxs, metricName),
        label: `T(${metricName})`,
      };
    }
  })();

  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label,
    renderer: (row: Readonly<Row>) => {
      if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
        return (
          <MetricCell
            value={getValues([row.arrow_pair.points_to])[0] as number}
            format={metricSettings?.format}
          />
        );
      } else {
        return <MissingMetric />;
      }
    },
    getNumericValues: getValues,
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function createTieredTransitiveMetricColumn(
  metricName: string,
  nativeGraph: NativeGraph,
  metricSettings: MetricSettings | null,
  tvc: TraversalConfig,
  dominated: boolean,
): { columnID: string; definition: NumericValueColumnDefinition }[] {
  const graphHasMoreThanOneMetric = nativeGraph.metricNames.length > 1;
  const tiers = nativeGraph.stats().tier_names;
  const maxTier = tvc.tiered_traversal?.AscendingTiers?.max_tier;

  return tiers.flatMap((tierName, tier_idx) => {
    if (metricSettings?.column_show_tiered == null && maxTier != null) {
      if (tier_idx > maxTier) {
        return [];
      }
    }

    if (metricSettings?.column_show_tiered?.[tierName] === "Never") {
      return [];
    }

    const [columnID, label, getValue] = (() => {
      if (dominated) {
        const columnID = `[tiered_transitive dominated ${tierName}] ${metricName}`;
        const label = graphHasMoreThanOneMetric
          ? `D(${metricName}(${tierName}))`
          : `D(${tierName})`;
        const getValue = (idxs: NodeIDX[]) =>
          nativeGraph
            .getTieredTransitiveMetricsDominatedBatched(idxs, metricName)
            .map((m) => m[tierName] ?? 0);

        return [columnID, label, getValue];
      } else {
        const columnID = `[tiered_transitive ${tierName}] ${metricName}`;
        const label = graphHasMoreThanOneMetric
          ? `${metricName}(${tierName})`
          : tierName;
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
      renderer: (row: Readonly<Row>) => {
        if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
          return (
            <MetricCell
              value={getValue([row.arrow_pair.points_to])[0] as number}
              format={metricSettings?.format}
            />
          );
        } else if (
          row.arrow_pair.points_from === ARROW_POINTS_FROM_NON_EXISTENT
        ) {
          // If the arrow is coming from a non-existent point, we don't know
          // what to override
          return <MissingMetric />;
        } else {
          const currentValue =
            nativeGraph.getCombinedMetricsForEntryPoints().tiered_metrics?.[
              metricName
            ]?.[tierName] ?? 0;
          const wouldBeValue =
            nativeGraph.getCombinedMetricsForEntryPointsWithOverrides({
              from: row.arrow_pair.points_from,
              to: row.arrow_pair.points_to,
            }).tiered_metrics?.[metricName]?.[tierName] ?? 0;
          return (
            <WouldBeDeltaMetricCell
              value={wouldBeValue - currentValue}
              format={metricSettings?.format}
            />
          );
        }
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
      renderer: (row: Readonly<Row>) => {
        if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
          return (
            <MetricCell
              value={getValues([row.arrow_pair.points_to])[0] as number}
              format={metricSettings?.format}
            />
          );
        } else {
          return <MissingMetric />;
        }
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
      renderer: (row: Readonly<Row>) => {
        if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
          return (
            <MetricCell
              value={getValues([row.arrow_pair.points_to])[0] as number}
              format={metricSettings?.format}
            />
          );
        } else {
          return <MissingMetric />;
        }
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
    renderer: (row: Readonly<Row>) => {
      if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
        return (
          <MetricCell
            value={
              nativeGraph.getParentsCount([row.arrow_pair.points_to])[0] ?? 0
            }
            format={NO_PRECISION_FORMAT}
          />
        );
      } else {
        return <MissingMetric />;
      }
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
  dominated: boolean,
  side: GraphSide,
  showWouldBe: boolean,
): [string, NumericValueColumnDefinition] {
  const columnID = `transitive_count_${side}`;
  const { getValues, label } = (() => {
    if (dominated) {
      return {
        getValues: (idxs: NodeIDX[]) =>
          nativeGraph.getTransitiveCountDominated(idxs),
        label: "D(count)",
      };
    } else {
      return {
        getValues: (idxs: NodeIDX[]) => nativeGraph.getTransitiveCount(idxs),
        label: "T(count)",
      };
    }
  })();

  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: label,
    renderer: (row: Readonly<Row>) => {
      if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
        return (
          <MetricCell
            value={getValues([row.arrow_pair.points_to])[0] ?? 0}
            format={NO_PRECISION_FORMAT}
          />
        );
      } else if (
        row.arrow_pair.points_from === ARROW_POINTS_FROM_NON_EXISTENT
      ) {
        // If the arrow is coming from a non-existent point, we don't know
        // what to override
        return <MissingMetric />;
      } else {
        const currentValue =
          nativeGraph.getCombinedMetricsForEntryPoints().node_count ?? 0;
        const wouldBeValue =
          nativeGraph.getCombinedMetricsForEntryPointsWithOverrides({
            from: row.arrow_pair.points_from,
            to: row.arrow_pair.points_to,
          }).node_count ?? 0;
        return showWouldBe ? (
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
    sortable: true,
    isHidden: false,
  };
  return [columnID, definition];
}

function createTransitiveCountDeltaColumn(
  nativeGraphL: NativeGraph,
  nativeGraphR: NativeGraph,
  dominated: boolean,
): [string, NumericValueColumnDefinition] {
  const columnID = "transitive_count_delta";
  const { getValues, label } = (() => {
    if (dominated) {
      return {
        getValues: (idxs: NodeIDX[]) => {
          return diffMetricArrays(
            nativeGraphL.getTransitiveCountDominated(idxs),
            nativeGraphR.getTransitiveCountDominated(idxs),
          );
        },
        label: "D(count) ∆",
      };
    } else {
      return {
        getValues: (idxs: NodeIDX[]) =>
          diffMetricArrays(
            nativeGraphL.getTransitiveCount(idxs),
            nativeGraphR.getTransitiveCount(idxs),
          ),
        label: "T(count) ∆",
      };
    }
  })();

  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: label,
    renderer: (row: Readonly<Row>) => {
      if (
        nativeGraphL.isNodeReachable(row.arrow_pair.points_to) ||
        nativeGraphR.isNodeReachable(row.arrow_pair.points_to)
      ) {
        return (
          <MetricCell
            value={getValues([row.arrow_pair.points_to])[0] ?? 0}
            format={NO_PRECISION_FORMAT}
          />
        );
      } else {
        // If the arrow is coming from a non-existent point, we don't know
        // what to override
        return <MissingMetric />;
      }
    },
    getNumericValues: getValues,
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
    renderer: (row: Readonly<Row>) => {
      if (nativeGraph.isNodeReachable(row.arrow_pair.points_to)) {
        return (
          <MetricCell
            value={
              nativeGraph.getConjointCost().count[row.arrow_pair.points_to] ?? 0
            }
            format={NO_PRECISION_FORMAT}
          />
        );
      } else {
        return <MissingMetric />;
      }
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
          "px-2 mx-2 py-1 rounded-md text-right tabular-nums w-full whitespace-nowrap bg-background",
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

/// Diff two arrays of the same length.
/// used for computing delta column values across multiple rows.
///
/// e.g. [1, 10, 0]
/// and. [0, 15, 2]
/// will make:
///      [-1, 5, 2]
function diffMetricArrays(a: Float32Array, b: Float32Array): Float32Array {
  const maxLen = Math.max(a.length, b.length);
  const result = new Float32Array(maxLen);
  for (let i = 0; i < maxLen; i++) {
    result[i] = (b[i] ?? 0) - (a[i] ?? 0);
  }
  return result;
}

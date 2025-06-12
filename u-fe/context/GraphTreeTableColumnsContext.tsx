// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import type { NodeIDX } from "u-be/unigraph_core/bindings/NodeIDX";
import type NativeGraph from "../NativeGraph";
import { formatPlainNumber } from "../lib/formatNumber";
import ContextMenuCell from "../tree_table/ContextMenuCell";
import type {
  ColumnDefinitions,
  NonTreeColumnDefinition,
  NumericValueColumnDefinition,
  TreeColumnDefinition,
} from "../tree_table/TreeTable";
import { useNativeGraph } from "./NativeGraphContext";

export type GraphTreeTableColumnsContextType = {
  columnDefinitions: ColumnDefinitions;
  setColumnDefinitions: (definitions: ColumnDefinitions) => void;
};

const GraphTreeTableColumnsContext =
  createContext<GraphTreeTableColumnsContextType | null>(null);

export function GraphTreeTableColumnsContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const nativeGraph = useNativeGraph();
  const [columnDefinitions, setColumnDefinitions] = useState(() =>
    defaultColumnDefinitions(nativeGraph),
  );

  useEffect(() => {
    // we need to reset the whole thing if the native grpah changes, because
    // the columns will otherwise have references to the old graph and
    // the values will not be correct.
    // In the future we need to defive the columns from graph UI/Metircs settings
    setColumnDefinitions(defaultColumnDefinitions(nativeGraph));
  }, [nativeGraph]);

  const setDefinitionsCb = useCallback((newDefinitions: ColumnDefinitions) => {
    setColumnDefinitions(newDefinitions);
  }, []);

  const value = useMemo(() => {
    return {
      columnDefinitions,
      setColumnDefinitions: setDefinitionsCb,
    };
  }, [columnDefinitions, setDefinitionsCb]);

  return (
    <GraphTreeTableColumnsContext.Provider value={value}>
      {children}
    </GraphTreeTableColumnsContext.Provider>
  );
}

export function useGraphTreeTableColumns(): GraphTreeTableColumnsContextType {
  const context = useContext(GraphTreeTableColumnsContext);
  if (!context) {
    throw new Error(
      "useGraphTreeTableColumns must be used within a GraphTreeTableColumnsContextProvider",
    );
  }
  return context;
}

function defaultColumnDefinitions(nativeGraph: NativeGraph): ColumnDefinitions {
  const columnDefinitions: { [name: string]: NonTreeColumnDefinition } = {};
  for (const metricName of nativeGraph.metricNames) {
    const [metricColumnID, metricColumnDefinition] = createMetricColumn(
      nativeGraph,
      metricName,
    );

    const [transitiveMetricColumnID, transitiveMetricColumnDefinition] =
      createTransitiveMetricColumn(metricName, nativeGraph);

    columnDefinitions[metricColumnID] = metricColumnDefinition;
    columnDefinitions[transitiveMetricColumnID] =
      transitiveMetricColumnDefinition;

    const tieredTransitiveColumns = createTieredTransitiveMetricColumn(
      metricName,
      nativeGraph,
    );

    for (const { columnID, definition } of tieredTransitiveColumns) {
      columnDefinitions[columnID] = definition;
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
): [string, NumericValueColumnDefinition] {
  const columnID = `[metric] ${metricName}`;
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: metricName,
    renderer: (arrow: Arrow) => {
      const value = arrow.points_to_unreachable
        ? "-"
        : formatPlainNumber(
            nativeGraph.getNodeMetric(arrow.points_to, metricName),
            0,
            true,
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
): [string, NumericValueColumnDefinition] {
  const columnID = `[transitive] ${metricName}`;
  const definition: NumericValueColumnDefinition = {
    t: "numeric_value_column",
    label: columnID,
    renderer: (arrow: Arrow) => {
      const value = arrow.points_to_unreachable
        ? "-"
        : formatPlainNumber(
            nativeGraph.getTransitiveMetric(arrow.points_to, metricName),
            0,
            true,
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
): { columnID: string; definition: NumericValueColumnDefinition }[] {
  const tiers = nativeGraph.stats().tier_names;
  return tiers.map((tierName) => {
    const columnID = `[tiered_transitive ${tierName}] ${metricName}`;
    const definition: NumericValueColumnDefinition = {
      t: "numeric_value_column",
      label: tierName,
      renderer: (arrow: Arrow) => {
        const value = arrow.points_to_unreachable
          ? "-"
          : formatPlainNumber(
              nativeGraph.getTieredTransitiveMetric(
                arrow.points_to,
                metricName,
              )?.[tierName] ?? 0,
              0,
              true,
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

    return {
      columnID,
      definition,
    };
  });
}

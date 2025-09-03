// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "../types";
import { TreeTable } from "./TreeTable";

import { useCallback, useMemo } from "react";
import ErrorBoundary from "../components/ErrorBoundary";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useNativeGraphL, useTwinGraph } from "../context/NativeGraphContext";
import { GRAPH_STRUCTURE } from "../native/NativeGraph";
import useGraphTreeTableColumns from "./useGraphTreeTableColumns";

export default function GraphTreeTable(props: {
  roots: readonly NodeIDX[];
  focusOnMount?: boolean;
}) {
  const nativeGraphL = useNativeGraphL();
  const twinGraph = useTwinGraph();
  const [settings] = useGraphSettings();

  const columnDefinitions = useGraphTreeTableColumns();
  const graphStructure = settings.ui_settings?.graph_structure ?? "Forward";
  const treeTableEntryPoints =
    settings.ui_settings?.entry_points ?? "Determine";

  const getArrowPairs = useCallback(
    (nodeIDX: NodeIDX) => {
      switch (graphStructure) {
        case "Forward":
          return twinGraph.getArrowPairs(nodeIDX, GRAPH_STRUCTURE.FORWARD);
        case "Dominator":
          return twinGraph.getArrowPairs(nodeIDX, GRAPH_STRUCTURE.DOMINATOR);
        case "Reverse":
          return twinGraph.getArrowPairs(nodeIDX, GRAPH_STRUCTURE.REVERSE);
        default: {
          const _exhaustiveCheck: never = graphStructure;
          throw new Error(`Unknown column type: ${_exhaustiveCheck}`);
        }
      }
    },
    [twinGraph, graphStructure],
  );

  const getShortestPath = useCallback(
    (fromNodeIDX: readonly NodeIDX[], toNodeIDX: NodeIDX) => {
      const configuredPath = nativeGraphL.getShortestPath(
        fromNodeIDX,
        toNodeIDX,
        graphStructure,
        "Configured",
      );

      // first try to find shortest configured path to the graph to prioritize
      // reachable nodes.
      if (configuredPath != null && configuredPath.length !== 0) {
        return configuredPath;
      }

      // if the are no paths between nodes in the configured graph resort to
      // unconfigured shortest path, which (if exists) will go though
      // excluded edges.
      const unconfiguredPath = nativeGraphL.getShortestPath(
        fromNodeIDX,
        toNodeIDX,
        graphStructure,
        "Unconfigured",
      );

      return unconfiguredPath;
    },
    [nativeGraphL, graphStructure],
  );

  const treeTableGraph = useMemo(() => {
    return {
      getArrowPairs,
      roots: props.roots,
      getShortestPath,
      graphStructure,
      treeTableEntryPoints,
      isDeltaGraph: twinGraph.isDeltaGraph(),
    };
  }, [
    twinGraph,
    props.roots,
    getArrowPairs,
    graphStructure,
    getShortestPath,
    treeTableEntryPoints,
  ]);

  return (
    <ErrorBoundary>
      <TreeTable
        columnDefinitions={columnDefinitions}
        treeTableGraph={treeTableGraph}
        focusOnMount={props.focusOnMount}
      />
    </ErrorBoundary>
  );
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import ErrorBoundary from "../components/ErrorBoundary";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useTwinGraph } from "../context/NativeGraphContext";
import { GRAPH_STRUCTURE } from "../native/NativeGraph";
import type { NodeIDX } from "../types";
import { TreeTable } from "./TreeTable";
import useGraphTreeTableColumns from "./useGraphTreeTableColumns";

export default function GraphTreeTable(props: {
  roots: readonly NodeIDX[];
  focusOnMount?: boolean;
}) {
  const twinGraph = useTwinGraph();
  const [settings] = useGraphSettings();

  const columnDefinitions = useGraphTreeTableColumns();
  const graphStructure = settings.ui_settings?.graph_structure ?? "Forward";
  const treeTableEntryPoints =
    settings.ui_settings?.entry_points ?? "Determine";
  const changedNodesOnly =
    settings.ui_settings?.show_changed_nodes_only === "WhenRightGraphPresent" &&
    twinGraph.r != null;

  const getTwinArrows = useCallback(
    (nodeIDX: NodeIDX) => {
      switch (graphStructure) {
        case "Forward":
          return twinGraph.getTwinArrows(
            nodeIDX,
            GRAPH_STRUCTURE.FORWARD,
            changedNodesOnly,
          );
        case "Dominator":
          return twinGraph.getTwinArrows(
            nodeIDX,
            GRAPH_STRUCTURE.DOMINATOR,
            changedNodesOnly,
          );
        case "Reverse":
          return twinGraph.getTwinArrows(
            nodeIDX,
            GRAPH_STRUCTURE.REVERSE,
            changedNodesOnly,
          );
        default: {
          const _exhaustiveCheck: never = graphStructure;
          throw new Error(`Unknown column type: ${_exhaustiveCheck}`);
        }
      }
    },
    [twinGraph, graphStructure, changedNodesOnly],
  );

  const getShortestPath = useCallback(
    (fromNodeIDX: readonly NodeIDX[], toNodeIDX: NodeIDX) => {
      const configuredPath = twinGraph.getShortestPath(
        fromNodeIDX,
        toNodeIDX,
        graphStructure,
        "Configured",
        changedNodesOnly,
      );

      // first try to find shortest configured path to the graph to prioritize
      // reachable nodes.
      if (configuredPath != null && configuredPath.length !== 0) {
        return configuredPath;
      }

      // if the are no paths between nodes in the configured graph resort to
      // unconfigured shortest path, which (if exists) will go though
      // excluded edges.
      const unconfiguredPath = twinGraph.getShortestPath(
        fromNodeIDX,
        toNodeIDX,
        graphStructure,
        "Unconfigured",
        changedNodesOnly,
      );

      return unconfiguredPath;
    },
    [twinGraph, graphStructure, changedNodesOnly],
  );

  const treeTableGraph = useMemo(() => {
    return {
      getTwinArrows,
      roots: props.roots,
      getShortestPath,
      graphStructure,
      treeTableEntryPoints,
      isDeltaGraph: twinGraph.isDeltaGraph(),
    };
  }, [
    twinGraph,
    props.roots,
    getTwinArrows,
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

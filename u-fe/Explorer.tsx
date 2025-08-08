// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo, useRef } from "react";
import {
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import ExplorerFooter from "./ExplorerFooter";
import { useExplorerKeyboardShortcuts } from "./ExplorerKeyboardShortcutsWrapper";
import NativeGraph, { GRAPH_SIDE, type GraphSide } from "./NativeGraph";
import Sidebar from "./Sidebar";
import Simulation from "./Simulation";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { ExplorerComponentInputGraph } from "./__generated__/ts/ExplorerComponentInputGraph";
import type { ExplorerParams } from "./__generated__/ts/ExplorerParams";
import type { GraphSettings } from "./__generated__/ts/GraphSettings";
import type { TraversalConfig } from "./__generated__/ts/TraversalConfig";
import ErrorBoundary from "./components/ErrorBoundary";
import { PortalContextProvider } from "./components/PortalContext";
import {
  GraphSettingsContextProvider,
  useGraphSettings,
} from "./context/GraphSettingsContext";
import {
  NativeGraphContextProvider,
  useNativeGraph,
} from "./context/NativeGraphContext";
import {
  SelectedNodesContextProvider,
  useSelectedNodes,
} from "./context/SelectedNodesContext";
import { SelectedPathContextProvider } from "./context/SelectedPathContext";
import { SimulationParamsContextProvider } from "./context/SimulationParamsContext";
import { TraversalConfigContextProvider } from "./context/TraversalConfigContext";
import initWasm from "./init_wasm";
import GraphInfoPanel from "./sidebar_panels/GraphInfoPanel";
import GraphTreeTable from "./tree_table/GraphTreeTable";
import type { NodeIDX } from "./types";

export function Explorer({
  params,
}: {
  params: ExplorerParams;
}) {
  initWasm();

  const {
    graph_left,
    graph_right: _graph_right,
    traversal_config,
    on_traversal_config_change,
    graph_settings,
    on_graph_settings_change,
  } = params;

  const containerRef = useRef<HTMLDivElement>(null);

  /// This graph initializes a new native graph every time the raw data changes.
  const nativeGraphNoTVC = useMemo(
    () => initNativeGraph(graph_left, GRAPH_SIDE.L),
    [graph_left],
  );

  /// This hook will NOT re-initialize the native graph if the traversal config changes.
  /// We modify it in place and return a new nativeGraph reference with all caches busted.
  const [tvc, nativeGraph] = useMemo(() => {
    const tvc: TraversalConfig =
      traversal_config == null
        ? nativeGraphNoTVC.getTraversalConfig()
        : JSON.parse(from_zstd_base64_url_safe_no_pad(traversal_config));

    return [tvc, nativeGraphNoTVC.getApplyTraversalConfig(tvc)];
  }, [traversal_config, nativeGraphNoTVC]);

  const settings = useMemo(() => {
    return graph_settings == null
      ? nativeGraphNoTVC.getGraphSettings() // default settings come from the native graph
      : JSON.parse(from_zstd_base64_url_safe_no_pad(graph_settings));
  }, [graph_settings, nativeGraphNoTVC]);

  const setTvcCb = useCallback(
    (tvc: TraversalConfig) => {
      const traversal_config_zstd_base64_url_safe_no_padding =
        to_zstd_base64_url_safe_no_pad(JSON.stringify(tvc));

      on_traversal_config_change?.(
        traversal_config_zstd_base64_url_safe_no_padding,
      );
    },
    [on_traversal_config_change],
  );

  const setSettingsCb = useCallback(
    (settings: GraphSettings) => {
      const base64 = to_zstd_base64_url_safe_no_pad(JSON.stringify(settings));
      on_graph_settings_change?.(base64);
    },
    [on_graph_settings_change],
  );

  return (
    <div className="h-screen flex flex-col unigraph-explorer bg-background">
      <PortalContextProvider containerRef={containerRef}>
        <ErrorBoundary>
          <NativeGraphContextProvider nativeGraph={nativeGraph}>
            <TraversalConfigContextProvider tvc={tvc} setTvc={setTvcCb}>
              <SimulationParamsContextProvider>
                <SelectedNodesContextProvider>
                  <GraphSettingsContextProvider
                    settings={settings}
                    setSettings={setSettingsCb}
                  >
                    <SelectedPathContextProvider syncToURL={true}>
                      <Page containerRef={containerRef} />
                    </SelectedPathContextProvider>
                  </GraphSettingsContextProvider>
                </SelectedNodesContextProvider>
              </SimulationParamsContextProvider>
            </TraversalConfigContextProvider>
          </NativeGraphContextProvider>
        </ErrorBoundary>
      </PortalContextProvider>
    </div>
  );
}

function Page(props: {
  containerRef?: React.RefObject<HTMLDivElement | null>;
}) {
  const nativeGraph = useNativeGraph();
  const [graphSettings] = useGraphSettings();
  const [settings] = useGraphSettings();
  const [selectedNodes] = useSelectedNodes();
  const keyboardEventHandler = useExplorerKeyboardShortcuts();

  const selectedSidebarPanel =
    settings.ui_settings?.selected_sidebar_panel ?? "Simulation";

  const panelTab: React.ReactNode = (() => {
    switch (selectedSidebarPanel) {
      case "Simulation":
        return <Simulation />;
      case "None":
        return null;
      case "GraphInfo":
        return <GraphInfoPanel />;
      default: {
        const exhaustiveCheck: never = selectedSidebarPanel;
        throw new Error(`Unexpected panel tab: ${exhaustiveCheck}`);
      }
    }
  })();

  const roots = useMemo(() => {
    return getRoots(
      nativeGraph,
      selectedNodes,
      graphSettings.ui_settings?.entry_points_specified ?? null,
      graphSettings.ui_settings?.entry_points,
    );
  }, [
    nativeGraph,
    selectedNodes,
    graphSettings.ui_settings?.entry_points,
    graphSettings.ui_settings?.entry_points_specified,
  ]);

  return (
    <div
      className="flex grow-1 shrink flex-row bg-background text-foreground min-h-0"
      ref={props.containerRef}
      onKeyDown={keyboardEventHandler}
    >
      <Sidebar selectedPanelTab={selectedSidebarPanel} />
      {panelTab}
      <div className="flex flex-col h-full grow-1">
        <GraphTreeTable focusOnMount={true} roots={roots} />
        <ExplorerFooter />
      </div>
    </div>
  );
}

function initNativeGraph(
  graph: ExplorerComponentInputGraph,
  side: GraphSide,
): NativeGraph {
  if ("MapGraphSerialized" in graph) {
    graph.MapGraphSerialized;
    return NativeGraph.fromMapGraphJSON(graph.MapGraphSerialized.value, side);
  } else if ("ArrayGraphSerialized" in graph) {
    return NativeGraph.fromArrayGraphJSONZstdBase64(
      graph.ArrayGraphSerialized.value,
      side,
    );
  } else {
    const _: never = graph;
    _;
    throw new Error("Unhandled case");
  }
}

function getRoots(
  nativeGraph: NativeGraph,
  selectedNodes: NodeIDX[],
  entryPointsSpecified: string[] | null,
  entryPoints: ArrayGraphUISettingsTreeTableEntryPoints = "Determine",
): readonly NodeIDX[] {
  if (selectedNodes.length > 0) {
    return selectedNodes;
  }

  switch (entryPoints) {
    case "Determine":
      return nativeGraph.determineEntrypoints().vec;
    case "AllReachable":
      return nativeGraph.getAllReachableNodeIDXs().vec;
    case "Specified":
      if (entryPointsSpecified == null || entryPointsSpecified.length === 0) {
        return nativeGraph.determineEntrypoints().vec;
      }

      return entryPointsSpecified
        .map((idx) => nativeGraph.getNodeIDXByNameLog(idx))
        .filter((idx) => idx != null);
    default: {
      const exhaustiveCheck: never = entryPoints;
      throw new Error(
        `Unexpected entry points: ${JSON.stringify(exhaustiveCheck)}`,
      );
    }
  }
}

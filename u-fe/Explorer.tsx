// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo, useRef } from "react";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "u-be/unigraph_core/bindings/ArrayGraphUISettingsTreeTableEntryPoints";
import type { TraversalConfig } from "u-be/unigraph_core/bindings/TraversalConfig";
import {
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import type { GraphSettings } from "../u-be/unigraph_core/bindings/GraphSettings";
import ExplorerFooter from "./ExplorerFooter";
import { useExplorerKeyboardShortcuts } from "./ExplorerKeyboardShortcutsWrapper";
import NativeGraph from "./NativeGraph";
import Sidebar from "./Sidebar";
import Simulation from "./Simulation";
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

export type InputGraph =
  | {
      t: "MapGraphJSON";
      mapGraphJSON: string;
    }
  | {
      t: "array_graph_json_zstd_base64";
      array_graph_json_zstd_base64: string;
    };

export function Explorer({
  graph,
  traversalConfigZSTDBase64UrlSafeNoPadding,
  onTraversalConfigZSTDBase64UrlSafeNoPaddingChange,
  graphSettingsZSTDBase64UrlSafeNoPadding,
  onGraphSettingsZSTDBase64UrlSafeNoPaddingChange,
}: {
  graph: InputGraph;
  traversalConfigZSTDBase64UrlSafeNoPadding?: string | null;
  onTraversalConfigZSTDBase64UrlSafeNoPaddingChange?: (v: string) => void;
  graphSettingsZSTDBase64UrlSafeNoPadding?: string | null;
  onGraphSettingsZSTDBase64UrlSafeNoPaddingChange?: (v: string) => void;
}) {
  initWasm();

  const containerRef = useRef<HTMLDivElement>(null);

  /// This graph initializes a new native graph every time the raw data changes.
  const nativeGraphNoTVC = useMemo(() => initNativeGraph(graph), [graph]);

  /// This hook will NOT re-initialize the native graph if the traversal config changes.
  /// We modify it in place and return a new nativeGraph reference with all caches busted.
  const [tvc, nativeGraph] = useMemo(() => {
    const tvc: TraversalConfig =
      traversalConfigZSTDBase64UrlSafeNoPadding == null
        ? nativeGraphNoTVC.getTraversalConfig()
        : JSON.parse(
            from_zstd_base64_url_safe_no_pad(
              traversalConfigZSTDBase64UrlSafeNoPadding,
            ),
          );

    return [tvc, nativeGraphNoTVC.getApplyTraversalConfig(tvc)];
  }, [traversalConfigZSTDBase64UrlSafeNoPadding, nativeGraphNoTVC]);

  const settings = useMemo(() => {
    return graphSettingsZSTDBase64UrlSafeNoPadding == null
      ? nativeGraphNoTVC.getGraphSettings() // default settings come from the native graph
      : JSON.parse(
          from_zstd_base64_url_safe_no_pad(
            graphSettingsZSTDBase64UrlSafeNoPadding,
          ),
        );
  }, [graphSettingsZSTDBase64UrlSafeNoPadding, nativeGraphNoTVC]);

  const setTvcCb = useCallback(
    (tvc: TraversalConfig) => {
      const traversal_config_zstd_base64_url_safe_no_padding =
        to_zstd_base64_url_safe_no_pad(JSON.stringify(tvc));

      onTraversalConfigZSTDBase64UrlSafeNoPaddingChange?.(
        traversal_config_zstd_base64_url_safe_no_padding,
      );
    },
    [onTraversalConfigZSTDBase64UrlSafeNoPaddingChange],
  );

  const setSettingsCb = useCallback(
    (settings: GraphSettings) => {
      const base64 = to_zstd_base64_url_safe_no_pad(JSON.stringify(settings));
      onGraphSettingsZSTDBase64UrlSafeNoPaddingChange?.(base64);
    },
    [onGraphSettingsZSTDBase64UrlSafeNoPaddingChange],
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

function initNativeGraph(graph: InputGraph): NativeGraph {
  switch (graph.t) {
    case "array_graph_json_zstd_base64":
      return NativeGraph.fromArrayGraphJSONZstdBase64(
        graph.array_graph_json_zstd_base64,
      );
    case "MapGraphJSON":
      return NativeGraph.fromMapGraphJSON(graph.mapGraphJSON);
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

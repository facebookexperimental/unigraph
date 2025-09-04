// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import {
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import ExplorerFooter from "./ExplorerFooter";
import Sidebar from "./Sidebar";
import Simulation from "./Simulation";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { ExplorerProps } from "./__generated__/ts/ExplorerProps";
import type { GraphSettings } from "./__generated__/ts/GraphSettings";
import type { TraversalConfig } from "./__generated__/ts/TraversalConfig";
import ErrorBoundary from "./components/ErrorBoundary";
import {
  GlobalElementRefsContextProvider,
  usePortalContainer,
} from "./context/GlobalElementRefs";

import { useGlobalKeyboardShortcuts } from "./context/GlobalKeyboardShortcutsContext";
import {
  GraphSettingsContextProvider,
  useGraphSettings,
} from "./context/GraphSettingsContext";
import {
  NativeGraphContextProvider,
  useNativeGraphs,
} from "./context/NativeGraphContext";
import {
  SelectedNodesContextProvider,
  useSelectedNodes,
} from "./context/SelectedNodesContext";
import { SelectedPathContextProvider } from "./context/SelectedPathContext";
import { SimulationParamsContextProvider } from "./context/SimulationParamsContext";
import { TraversalConfigContextProvider } from "./context/TraversalConfigContext";
import initWasm from "./init_wasm";
import NativeGraph from "./native/NativeGraph";
import GraphInfoPanel from "./sidebar_panels/GraphInfoPanel";
import GraphTreeTable from "./tree_table/GraphTreeTable";
import type { NodeIDX } from "./types";

export function Explorer(props: ExplorerProps) {
  initWasm();

  const {
    graphs,
    traversal_config_l,
    on_traversal_config_change_l,
    traversal_config_r,
    on_traversal_config_change_r: _onTraversalConfigChangeR,
    graph_settings,
    on_graph_settings_change,
  } = props;

  /// This graph initializes a new native graph every time the raw data changes.
  const [nativeGraphNoTVCL, nativeGraphNoTVCR] = useMemo(
    () => NativeGraph.fromSerialized(graphs),
    [graphs],
  );

  /// This hook will NOT re-initialize the native graph if the traversal config changes.
  /// We modify it in place and return a new nativeGraph reference with all caches busted.
  const [tvcL, nativeGraphL] = useMemo(() => {
    const tvc: TraversalConfig =
      traversal_config_l == null
        ? nativeGraphNoTVCL.getTraversalConfig()
        : JSON.parse(from_zstd_base64_url_safe_no_pad(traversal_config_l));

    return [tvc, nativeGraphNoTVCL.getApplyTraversalConfig(tvc)];
  }, [traversal_config_l, nativeGraphNoTVCL]);

  const [_tvcR, nativeGraphR] = useMemo(() => {
    if (nativeGraphNoTVCR == null) {
      return [null, null];
    }
    const tvc: TraversalConfig =
      traversal_config_r == null
        ? nativeGraphNoTVCR.getTraversalConfig()
        : JSON.parse(from_zstd_base64_url_safe_no_pad(traversal_config_r));

    return [tvc, nativeGraphNoTVCR.getApplyTraversalConfig(tvc)];
  }, [traversal_config_r, nativeGraphNoTVCR]);

  const settings = useMemo(() => {
    return graph_settings == null
      ? nativeGraphNoTVCL.getGraphSettings() // default settings come from the native graph
      : JSON.parse(from_zstd_base64_url_safe_no_pad(graph_settings));
  }, [graph_settings, nativeGraphNoTVCL]);

  const setTvcCb = useCallback(
    (tvc: TraversalConfig) => {
      const traversal_config_zstd_base64_url_safe_no_padding =
        to_zstd_base64_url_safe_no_pad(JSON.stringify(tvc));

      on_traversal_config_change_l?.(
        traversal_config_zstd_base64_url_safe_no_padding,
      );
    },
    [on_traversal_config_change_l],
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
      <GlobalElementRefsContextProvider>
        <ErrorBoundary>
          <NativeGraphContextProvider
            nativeGraphL={nativeGraphL}
            nativeGraphR={nativeGraphR}
          >
            <TraversalConfigContextProvider tvc={tvcL} setTvc={setTvcCb}>
              <SimulationParamsContextProvider>
                <SelectedNodesContextProvider>
                  <GraphSettingsContextProvider
                    settings={settings}
                    setSettings={setSettingsCb}
                  >
                    <SelectedPathContextProvider syncToURL={true}>
                      <Page />
                    </SelectedPathContextProvider>
                  </GraphSettingsContextProvider>
                </SelectedNodesContextProvider>
              </SimulationParamsContextProvider>
            </TraversalConfigContextProvider>
          </NativeGraphContextProvider>
        </ErrorBoundary>
      </GlobalElementRefsContextProvider>
    </div>
  );
}

function Page() {
  const [nativeGraphL, nativeGraphR] = useNativeGraphs();
  const [graphSettings] = useGraphSettings();
  const [settings] = useGraphSettings();
  const [selectedNodes] = useSelectedNodes();
  const portalRef = usePortalContainer();
  useGlobalKeyboardShortcuts();

  const selectedSidebarPanel =
    settings.ui_settings?.selected_sidebar_panel ?? "None";

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
    const rootsL = getRoots(
      nativeGraphL,
      selectedNodes,
      graphSettings.ui_settings?.entry_points_specified ?? null,
      graphSettings.ui_settings?.entry_points,
    );

    if (nativeGraphR == null) {
      return rootsL;
    } else {
      const rootsR = getRoots(
        nativeGraphR,
        selectedNodes,
        graphSettings.ui_settings?.entry_points_specified ?? null,
        graphSettings.ui_settings?.entry_points,
      );
      return Array.from(new Set([...rootsL, ...rootsR]));
    }
  }, [
    nativeGraphL,
    nativeGraphR,
    selectedNodes,
    graphSettings.ui_settings?.entry_points,
    graphSettings.ui_settings?.entry_points_specified,
  ]);

  return (
    <div
      className="flex grow-1 shrink flex-row bg-background text-foreground min-h-0"
      ref={portalRef}
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

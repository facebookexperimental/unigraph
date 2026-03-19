// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import {
  apply_gqc_delta,
  derive_gqc_delta,
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { ExplorerProps } from "./__generated__/ts/ExplorerProps";
import type { GraphQueryConfig } from "./__generated__/ts/GraphQueryConfig";
import type { GraphSettings } from "./__generated__/ts/GraphSettings";
import type { TraversalConfig } from "./__generated__/ts/TraversalConfig";
import ErrorBoundary from "./components/ErrorBoundary";
import { DebugModeContextProvider } from "./context/DebugModeContext";
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
import ExplorerFooter from "./ExplorerFooter";
import initWasm from "./init_wasm";
import NativeGraph from "./native/NativeGraph";
import Sidebar from "./Sidebar";
import Simulation from "./Simulation";
import GraphInfoPanel from "./sidebar_panels/GraphInfoPanel";
import TraversalConfigEditorPanel from "./sidebar_panels/TraversalConfigEditorPanel";
import GraphTreeTable from "./tree_table/GraphTreeTable";
import type { NodeIDX } from "./types";

export function Explorer(props: ExplorerProps) {
  initWasm();

  const {
    graphs,
    base_gqc_l,
    base_gqc_r,
    gqc_delta_l,
    on_gqc_delta_change_l,
    gqc_delta_r,
    on_gqc_delta_change_r,
    graph_settings,
    on_graph_settings_change,
    home_href,
  } = props;

  /// This graph initializes a new native graph every time the raw data changes.
  const [nativeGraphNoTVCL, nativeGraphNoTVCR] = useMemo(
    () => NativeGraph.fromSerialized(graphs),
    [graphs],
  );

  // Resolve base GQC — either from props (handle mode) or from graph defaults (local mode)
  const resolvedBaseGqcL = useMemo(
    () => resolveBaseGqc(base_gqc_l, nativeGraphNoTVCL),
    [base_gqc_l, nativeGraphNoTVCL],
  );

  const resolvedBaseGqcR = useMemo(
    () =>
      nativeGraphNoTVCR != null
        ? resolveBaseGqc(base_gqc_r, nativeGraphNoTVCR)
        : null,
    [base_gqc_r, nativeGraphNoTVCR],
  );

  // Apply delta to get current GQC, then extract TVC
  const [tvcL, nativeGraphL] = useMemo(() => {
    const currentGqc = applyDelta(resolvedBaseGqcL, gqc_delta_l);
    const tvc: TraversalConfig =
      currentGqc.traversal_config ?? nativeGraphNoTVCL.getTraversalConfig();
    return [tvc, nativeGraphNoTVCL.getApplyTraversalConfig(tvc)];
  }, [resolvedBaseGqcL, gqc_delta_l, nativeGraphNoTVCL]);

  const [tvcR, nativeGraphR] = useMemo(() => {
    if (nativeGraphNoTVCR == null || resolvedBaseGqcR == null) {
      return [null, null];
    }
    const currentGqc = applyDelta(resolvedBaseGqcR, gqc_delta_r);
    const tvc: TraversalConfig =
      currentGqc.traversal_config ?? nativeGraphNoTVCR.getTraversalConfig();
    return [tvc, nativeGraphNoTVCR.getApplyTraversalConfig(tvc)];
  }, [resolvedBaseGqcR, gqc_delta_r, nativeGraphNoTVCR]);

  const settings = useMemo(() => {
    return graph_settings == null
      ? nativeGraphNoTVCL.getGraphSettings() // default settings come from the native graph
      : JSON.parse(from_zstd_base64_url_safe_no_pad(graph_settings));
  }, [graph_settings, nativeGraphNoTVCL]);

  const setTvcLCb = useCallback(
    (tvc: TraversalConfig) => {
      const modified: GraphQueryConfig = {
        ...resolvedBaseGqcL,
        traversal_config: tvc,
      };
      const delta = derive_gqc_delta(
        JSON.stringify(resolvedBaseGqcL),
        JSON.stringify(modified),
      );
      on_gqc_delta_change_l?.(delta);
    },
    [resolvedBaseGqcL, on_gqc_delta_change_l],
  );

  const setTvcRCb = useCallback(
    (tvc: TraversalConfig) => {
      if (nativeGraphR == null || resolvedBaseGqcR == null) {
        return;
      }
      const modified: GraphQueryConfig = {
        ...resolvedBaseGqcR,
        traversal_config: tvc,
      };
      const delta = derive_gqc_delta(
        JSON.stringify(resolvedBaseGqcR),
        JSON.stringify(modified),
      );
      on_gqc_delta_change_r?.(delta);
    },
    [resolvedBaseGqcR, on_gqc_delta_change_r, nativeGraphR],
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
      <DebugModeContextProvider>
        <GlobalElementRefsContextProvider>
          <ErrorBoundary>
            <NativeGraphContextProvider
              nativeGraphL={nativeGraphL}
              nativeGraphR={nativeGraphR}
            >
              <TraversalConfigContextProvider
                tvcL={tvcL}
                setTvcL={setTvcLCb}
                tvcR={tvcR}
                setTvcR={setTvcRCb}
              >
                <SimulationParamsContextProvider>
                  <SelectedNodesContextProvider>
                    <GraphSettingsContextProvider
                      settings={settings}
                      setSettings={setSettingsCb}
                    >
                      <SelectedPathContextProvider syncToURL={true}>
                        <Page homeHref={home_href} />
                      </SelectedPathContextProvider>
                    </GraphSettingsContextProvider>
                  </SelectedNodesContextProvider>
                </SimulationParamsContextProvider>
              </TraversalConfigContextProvider>
            </NativeGraphContextProvider>
          </ErrorBoundary>
        </GlobalElementRefsContextProvider>
      </DebugModeContextProvider>
    </div>
  );
}

function Page({ homeHref }: { homeHref?: string }) {
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
      case "TraversalConfigEditor":
        return <TraversalConfigEditorPanel />;
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
      <Sidebar selectedPanelTab={selectedSidebarPanel} homeHref={homeHref} />
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

function resolveBaseGqc(
  baseGqcJson: string | undefined,
  nativeGraph: NativeGraph,
): GraphQueryConfig {
  if (baseGqcJson != null) {
    return JSON.parse(baseGqcJson);
  }
  // Local mode: construct a base GQC from graph defaults
  return {
    roots: [],
    traversal_config: nativeGraph.getTraversalConfig(),
  };
}

function applyDelta(
  base: GraphQueryConfig,
  deltaBase64: string | undefined,
): GraphQueryConfig {
  if (deltaBase64 == null || deltaBase64 === "") {
    return base;
  }
  const resultJson = apply_gqc_delta(JSON.stringify(base), deltaBase64);
  return JSON.parse(resultJson);
}

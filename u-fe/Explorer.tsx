// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo, useRef, type ReactNode } from "react";
import { Info, SlidersHorizontal, Waypoints } from "lucide-react";
import {
  apply_gqc_delta,
  derive_gqc_delta,
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { GraphQueryConfig } from "./__generated__/ts/GraphQueryConfig";
import type { GraphSettings } from "./__generated__/ts/GraphSettings";
import type { TraversalConfig } from "./__generated__/ts/TraversalConfig";

// ---------------------------------------------------------------------------
// Exported types — these end up in the bundled .d.ts for external consumers.
// ---------------------------------------------------------------------------

export type SerializationFormat =
  | "Json"
  | "JsonZstdBase64"
  | "JsonZstdFastBase64"
  | "JsonZstdBestBase64"
  | "JsonZstdBase64URLSafeNoPad"
  | "JsonZstdBestBase64URLSafeNoPad"
  | "JsonZstdFastBase64URLSafeNoPad";

export interface SerializedStr {
  data: string;
  format: SerializationFormat;
  type_hint?: string | undefined;
}

export type ExplorerComponentInputGraph =
  | { MapGraphSerialized: SerializedStr }
  | { ArrayGraphSerialized: SerializedStr }
  | { ArrayGraphSerializedPackageBase64: SerializedStr };

export type ExplorerComponentInputGraphVariants =
  | "MapGraphSerialized"
  | "ArrayGraphSerialized"
  | "ArrayGraphSerializedPackageBase64";

export interface ExplorerComponentInputGraphs {
  left: ExplorerComponentInputGraph;
  right?: ExplorerComponentInputGraph | undefined;
}

export type CallbackFn = (value: string) => void;

export interface ExplorerConfig {
  base_gqc_l?: string | undefined;
  base_gqc_r?: string | undefined;
  gqc_delta_l?: string | undefined;
  on_gqc_delta_change_l?: CallbackFn | undefined;
  gqc_delta_r?: string | undefined;
  on_gqc_delta_change_r?: CallbackFn | undefined;
  graph_settings?: string | undefined;
  on_graph_settings_change: CallbackFn;
}

export type BuiltinSidebarPanel =
  | "Simulation"
  | "GraphInfo"
  | "TraversalConfigEditor";

export interface PanelTabPlugin {
  id: string;
  icon: ReactNode;
  tooltip?: string;
  render: () => ReactNode;
}

export interface ExplorerProps {
  graphs: ExplorerComponentInputGraphs;
  config: ExplorerConfig;
  home_href?: string | undefined;
  panels?: PanelTabPlugin[];
  hidden_panels?: BuiltinSidebarPanel[];
}

interface ResolvedPanel {
  id: string;
  icon: ReactNode;
  tooltip?: string;
  render: () => ReactNode;
}
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
    graphs: rawGraphs,
    config,
    home_href,
    panels: customPanels,
    hidden_panels,
  } = props;
  const {
    base_gqc_l,
    base_gqc_r,
    gqc_delta_l,
    on_gqc_delta_change_l,
    gqc_delta_r,
    on_gqc_delta_change_r,
    graph_settings,
    on_graph_settings_change,
  } = config;

  // Stabilize graphs reference so consumers don't need to memoize.
  // Only re-parses through WASM when the actual serialized data changes.
  const graphs = useStableGraphs(rawGraphs);

  const [nativeGraphNoTVCL, nativeGraphNoTVCR] = useMemo(
    () => NativeGraph.fromSerialized(graphs),
    [graphs],
  );

  const left = useResolvedSide(
    base_gqc_l,
    gqc_delta_l,
    on_gqc_delta_change_l,
    nativeGraphNoTVCL,
  );
  const right = useResolvedSide(
    base_gqc_r,
    gqc_delta_r,
    on_gqc_delta_change_r,
    nativeGraphNoTVCR ?? null,
  );

  const BUILTIN_PANELS: ResolvedPanel[] = useMemo(
    () => [
      {
        id: "Simulation",
        icon: <Waypoints />,
        tooltip: "Simulation",
        render: () => <Simulation />,
      },
      {
        id: "GraphInfo",
        icon: <Info />,
        tooltip: "Graph Info",
        render: () => <GraphInfoPanel />,
      },
      {
        id: "TraversalConfigEditor",
        icon: <SlidersHorizontal />,
        tooltip: "Traversal Config",
        render: () => <TraversalConfigEditorPanel />,
      },
    ],
    [],
  );

  const resolvedPanels = useMemo(() => {
    const hiddenSet = new Set(hidden_panels ?? []);
    const builtins = BUILTIN_PANELS.filter(
      (p) => !hiddenSet.has(p.id as BuiltinSidebarPanel),
    );
    return [...builtins, ...(customPanels ?? [])];
  }, [BUILTIN_PANELS, customPanels, hidden_panels]);

  const settings = useMemo(() => {
    return graph_settings == null
      ? nativeGraphNoTVCL.getGraphSettings()
      : JSON.parse(from_zstd_base64_url_safe_no_pad(graph_settings));
  }, [graph_settings, nativeGraphNoTVCL]);

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
              nativeGraphL={left.nativeGraph!}
              nativeGraphR={right.nativeGraph}
            >
              <TraversalConfigContextProvider
                tvcL={left.tvc!}
                setTvcL={left.setTvc}
                tvcR={right.tvc}
                setTvcR={right.setTvc}
              >
                <SimulationParamsContextProvider>
                  <SelectedNodesContextProvider>
                    <GraphSettingsContextProvider
                      settings={settings}
                      setSettings={setSettingsCb}
                    >
                      <SelectedPathContextProvider syncToURL={true}>
                        <Page homeHref={home_href} panels={resolvedPanels} />
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

function Page({
  homeHref,
  panels,
}: {
  homeHref?: string;
  panels: ResolvedPanel[];
}) {
  const [nativeGraphL, nativeGraphR] = useNativeGraphs();
  const [settings] = useGraphSettings();
  const [selectedNodes] = useSelectedNodes();
  const portalRef = usePortalContainer();
  useGlobalKeyboardShortcuts();

  const selectedSidebarPanel =
    settings.ui_settings?.selected_sidebar_panel ?? "None";

  const panelTab =
    panels.find((p) => p.id === selectedSidebarPanel)?.render() ?? null;

  const roots = useMemo(() => {
    const rootsL = getRoots(
      nativeGraphL,
      selectedNodes,
      settings.ui_settings?.entry_points_specified ?? null,
      settings.ui_settings?.entry_points,
    );

    if (nativeGraphR == null) {
      return rootsL;
    } else {
      const rootsR = getRoots(
        nativeGraphR,
        selectedNodes,
        settings.ui_settings?.entry_points_specified ?? null,
        settings.ui_settings?.entry_points,
      );
      return Array.from(new Set([...rootsL, ...rootsR]));
    }
  }, [
    nativeGraphL,
    nativeGraphR,
    selectedNodes,
    settings.ui_settings?.entry_points,
    settings.ui_settings?.entry_points_specified,
  ]);

  return (
    <div
      className="flex grow-1 shrink flex-row bg-background text-foreground min-h-0"
      ref={portalRef}
    >
      <Sidebar
        selectedPanelTab={selectedSidebarPanel}
        homeHref={homeHref}
        panels={panels}
      />
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

function useResolvedSide(
  baseGqcJson: string | undefined,
  gqcDelta: string | undefined,
  onDeltaChange: ((delta: string) => void) | undefined,
  nativeGraphNoTVC: NativeGraph | null,
) {
  const resolvedBase = useMemo(
    () =>
      nativeGraphNoTVC != null
        ? resolveBaseGqc(baseGqcJson, nativeGraphNoTVC)
        : null,
    [baseGqcJson, nativeGraphNoTVC],
  );

  const [tvc, nativeGraph] = useMemo(() => {
    if (resolvedBase == null || nativeGraphNoTVC == null) {
      return [null, null];
    }
    const currentGqc = applyDelta(resolvedBase, gqcDelta);
    const tvc: TraversalConfig =
      currentGqc.traversal_config ?? nativeGraphNoTVC.getTraversalConfig();
    return [tvc, nativeGraphNoTVC.getApplyTraversalConfig(tvc)];
  }, [resolvedBase, gqcDelta, nativeGraphNoTVC]);

  const setTvc = useCallback(
    (tvc: TraversalConfig) => {
      if (resolvedBase == null) {
        return;
      }
      const modified: GraphQueryConfig = {
        ...resolvedBase,
        traversal_config: tvc,
      };
      const delta = derive_gqc_delta(
        JSON.stringify(resolvedBase),
        JSON.stringify(modified),
      );
      onDeltaChange?.(delta);
    },
    [resolvedBase, onDeltaChange],
  );

  return { tvc, nativeGraph, setTvc };
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

// Stabilize graphs object reference so consumers don't need to memoize.
// Compares the underlying serialized data strings rather than object identity,
// preventing expensive WASM re-parsing on unrelated re-renders.
function useStableGraphs(
  graphs: ExplorerComponentInputGraphs,
): ExplorerComponentInputGraphs {
  const ref = useRef(graphs);
  if (
    getGraphData(graphs.left) !== getGraphData(ref.current.left) ||
    getGraphData(graphs.right) !== getGraphData(ref.current.right)
  ) {
    ref.current = graphs;
  }
  return ref.current;
}

function getGraphData(
  g: ExplorerComponentInputGraph | undefined,
): string | undefined {
  if (g == null) return undefined;
  if ("MapGraphSerialized" in g) return g.MapGraphSerialized.data;
  if ("ArrayGraphSerialized" in g) return g.ArrayGraphSerialized.data;
  if ("ArrayGraphSerializedPackageBase64" in g)
    return g.ArrayGraphSerializedPackageBase64.data;
  return undefined;
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Suspense, useCallback, useMemo, useRef, type ReactNode } from "react";
import {
  QueryClient,
  QueryClientProvider,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { GraphLoadingAnimation } from "./components/GraphLoadingAnimation";
import {
  Bug,
  Download,
  Info,
  Scissors,
  SlidersHorizontal,
  Waypoints,
} from "lucide-react";
import {
  apply_gqc_delta,
  derive_gqc_delta,
} from "../.build/wasm/unigraph_wasm";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { NodeSelection } from "./__generated__/ts/NodeSelection";
import type { GraphQueryConfig } from "./__generated__/ts/GraphQueryConfig";
import type { GraphQueryOutput } from "./__generated__/ts/GraphQueryOutput";
import type { TraversalConfig } from "./__generated__/ts/TraversalConfig";
import type { TwinArrow } from "./__generated__/ts/TwinArrow";
import { useRpc, type UnigraphRpc } from "./api/rpc";

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
  left?: ExplorerComponentInputGraph | undefined;
  right: ExplorerComponentInputGraph;
}

export interface ExplorerConfig {
  base_gqc_l?: string | undefined;
  base_gqc_r?: string | undefined;
}

export type BuiltinSidebarPanel =
  | "Simulation"
  | "GraphInfo"
  | "TraversalConfigEditor"
  | "MinCut"
  | "DebugPanel"
  | "ExportGraph";

export interface PanelTabPlugin {
  id: string;
  icon: ReactNode;
  tooltip?: string;
  render: () => ReactNode;
}

// ---------------------------------------------------------------------------
// Plugins — optional UI extensions supplied by external consumers. Each plugin
// slot is rendered at a specific spot in the Explorer UI.
// ---------------------------------------------------------------------------

export interface TableNodeNameAfterProps {
  twinArrow: TwinArrow;
}

/**
 * Renders custom content in the tree table's node-name column, immediately
 * after the node name and the built-in debug/info icons. Return `null` to
 * render nothing for a given row.
 */
export type TableNodeNameAfterComponent =
  React.ComponentType<TableNodeNameAfterProps>;

export interface ExplorerPlugins {
  table_node_name_after_component?: TableNodeNameAfterComponent;
}

// ---------------------------------------------------------------------------
// Dynamic extensions — plugins/panels chosen once the handles have resolved.
//
// Consumers usually only know a handle up front, and it may be anonymous
// (`gqc_1a2b…`) or floating (`my-timeline` = latest). The timeline and graph
// IDs are only known after the graph is fetched, so anything that depends on
// *which* timeline is being explored has to be resolved at that point.
//
//   handle ──fetch──> graph_key ("my-timeline~223") ──resolve──> extensions
// ---------------------------------------------------------------------------

/** A graph handle plus the concrete snapshot the server resolved it to. */
export interface ResolvedGraphRef {
  /** The handle as passed in — possibly anonymous or floating. */
  handle: string;
  /** Canonical `"{timeline}~{graph_id}"` key of the snapshot actually loaded. */
  graph_key: string;
  timeline_id: string;
  graph_id: number;
}

export interface ResolvedGraphSource {
  right: ResolvedGraphRef;
  left?: ResolvedGraphRef;
}

/**
 * Everything a consumer can contribute to the Explorer UI. The same shape can
 * be passed statically through `ExplorerProps` or returned from
 * `resolve_extensions` — the two are merged, with the resolved ones winning on
 * per-slot conflicts.
 */
export interface ExplorerExtensions {
  plugins?: ExplorerPlugins;
  panels?: PanelTabPlugin[];
  hidden_panels?: BuiltinSidebarPanel[];
}

/**
 * Picks the extensions to use for a given resolved source. Called on every
 * render, so it must be cheap and pure — return referentially stable component
 * identities (module-level constants or a lookup table), not fresh closures,
 * or the panels will remount on each render.
 *
 * Both sides are supplied; in compare mode they may sit on different timelines,
 * so it is up to the consumer to decide which one drives the choice (usually
 * `right`).
 */
export type ExplorerExtensionsResolver = (
  source: ResolvedGraphSource,
) => ExplorerExtensions;

/**
 * A graph plus its overrides — structurally the generated `GraphQueryConfig`,
 * re-exported under the name the Explorer API uses. Do not redeclare this
 * shape; it has to stay in step with the Rust type the RPC expects.
 */
export type ExplorerGraphHandle = GraphQueryConfig;

export type ExplorerGraphSource = {
  type: "handle";
  right: ExplorerGraphHandle;
  left?: ExplorerGraphHandle;
};

export interface ExplorerProps {
  source: ExplorerGraphSource;
  config?: ExplorerConfig;
  home_href?: string | undefined;
  panels?: PanelTabPlugin[];
  hidden_panels?: BuiltinSidebarPanel[];
  plugins?: ExplorerPlugins;
  /**
   * Contributes further extensions once the handles have resolved to concrete
   * timelines. Merged on top of the static `plugins`/`panels`/`hidden_panels`.
   */
  resolve_extensions?: ExplorerExtensionsResolver;
  initialSearchParams?: Record<string, string>;
  onSearchParamsChange?: (params: Record<string, string>) => void;
}

interface ResolvedPanel {
  id: string;
  icon: ReactNode;
  tooltip?: string;
  render: () => ReactNode;
}
import ErrorBoundary from "./components/ErrorBoundary";
import {
  DebugModeContextProvider,
  useDebugMode,
} from "./context/DebugModeContext";
import {
  GlobalElementRefsContextProvider,
  usePortalContainer,
} from "./context/GlobalElementRefs";
import { useGlobalKeyboardShortcuts } from "./context/GlobalKeyboardShortcutsContext";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { MetricViewStateProvider } from "./context/MetricViewStateContext";
import { MinCutContextProvider } from "./context/MinCutContext";
import {
  SearchParamsProvider,
  useSearchParamsContext,
} from "./context/SearchParamsContext";
import {
  NativeGraphContextProvider,
  useNativeGraphs,
} from "./context/NativeGraphContext";
import { PluginsContextProvider } from "./context/PluginsContext";
import { ResolvedSourceContextProvider } from "./context/ResolvedSourceContext";
import { parseGraphKey } from "./lib/graphKey";
import {
  SelectedNodesContextProvider,
  useSelectedNodes,
} from "./context/SelectedNodesContext";
import { SelectedPathContextProvider } from "./context/SelectedPathContext";
import { SimulationParamsContextProvider } from "./context/SimulationParamsContext";
import { TraversalConfigContextProvider } from "./context/TraversalConfigContext";
import ExplorerFooter from "./ExplorerFooter";
import NativeGraph from "./native/NativeGraph";
import Sidebar from "./Sidebar";
import Simulation from "./Simulation";
import GraphInfoPanel from "./sidebar_panels/GraphInfoPanel";
import DebugPanel from "./sidebar_panels/DebugPanel";
import ExportGraphPanel from "./sidebar_panels/ExportGraphPanel";
import MinCutPanel from "./sidebar_panels/MinCutPanel";
import TraversalConfigEditorPanel from "./sidebar_panels/TraversalConfigEditorPanel";
import GraphTreeTable from "./tree_table/GraphTreeTable";
import type { NodeIDX } from "./types";

// Graphs are huge (tens of MB) — never auto-refetch or garbage-collect.
// Self-contained inside Explorer so UMD consumers don't need their own provider.
const explorerQueryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: Infinity,
      gcTime: Infinity,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
      refetchOnMount: false,
      retry: false,
    },
  },
});

export function Explorer(props: ExplorerProps) {
  return (
    <SearchParamsProvider
      initialSearchParams={props.initialSearchParams ?? {}}
      onParamsChange={props.onSearchParamsChange}
    >
      <QueryClientProvider client={explorerQueryClient}>
        <Suspense fallback={<GraphLoadingAnimation />}>
          <ExplorerFetcher {...props} />
        </Suspense>
      </QueryClientProvider>
    </SearchParamsProvider>
  );
}

// ---------------------------------------------------------------------------
// Fetcher — suspends while loading graph data from a source
// ---------------------------------------------------------------------------

function ExplorerFetcher(props: ExplorerProps) {
  const rpc = useRpc();
  const { plugins, panels, hidden_panels, resolve_extensions } = props;

  const { data: result } = useSuspenseQuery({
    queryKey: ["graphSource", props.source],
    queryFn: () => fetchGraphSource(rpc, props.source),
  });

  const extensions = useMemo(
    () =>
      mergeExtensions(
        { plugins, panels, hidden_panels },
        resolve_extensions?.(result.resolved),
      ),
    [plugins, panels, hidden_panels, resolve_extensions, result.resolved],
  );

  const baseGqcLJson = useMemo(
    () =>
      result.baseGqcL != null ? JSON.stringify(result.baseGqcL) : undefined,
    [result.baseGqcL],
  );
  const baseGqcRJson = useMemo(
    () =>
      result.baseGqcR != null ? JSON.stringify(result.baseGqcR) : undefined,
    [result.baseGqcR],
  );

  return (
    <ExplorerImpl
      {...props}
      {...extensions}
      graphs={result.graphs}
      resolvedSource={result.resolved}
      config={{
        ...props.config,
        base_gqc_l: baseGqcLJson ?? props.config?.base_gqc_l,
        base_gqc_r: baseGqcRJson ?? props.config?.base_gqc_r,
      }}
    />
  );
}

/** Static extensions first, resolved ones on top — last writer wins per slot. */
function mergeExtensions(
  base: ExplorerExtensions,
  resolved: ExplorerExtensions | undefined,
): ExplorerExtensions {
  if (resolved == null) {
    return base;
  }
  return {
    plugins: { ...base.plugins, ...resolved.plugins },
    panels: [...(base.panels ?? []), ...(resolved.panels ?? [])],
    hidden_panels: [
      ...(base.hidden_panels ?? []),
      ...(resolved.hidden_panels ?? []),
    ],
  };
}

// ---------------------------------------------------------------------------
// Implementation — renders the explorer UI given resolved graphs
// ---------------------------------------------------------------------------

function ExplorerImpl(props: {
  source: ExplorerGraphSource;
  graphs: ExplorerComponentInputGraphs;
  resolvedSource: ResolvedGraphSource;
  config?: ExplorerConfig;
  home_href?: string | undefined;
  panels?: PanelTabPlugin[];
  hidden_panels?: BuiltinSidebarPanel[];
  plugins?: ExplorerPlugins;
}) {
  const {
    source,
    graphs: rawGraphs,
    resolvedSource,
    config,
    home_href,
    panels: customPanels,
    hidden_panels,
    plugins,
  } = props;
  const { base_gqc_l, base_gqc_r } = config ?? {};

  // Stabilize graphs reference so consumers don't need to memoize.
  // Only re-parses through WASM when the actual serialized data changes.
  const graphs = useStableGraphs(rawGraphs);

  const [nativeGraphNoTVCL, nativeGraphNoTVCR] = useMemo(
    () => NativeGraph.fromSerialized(graphs),
    [graphs],
  );

  const left = useResolvedSide(
    base_gqc_l,
    "gqc_delta_left",
    nativeGraphNoTVCL ?? null,
  );
  const right = useResolvedSide(
    base_gqc_r,
    "gqc_delta_right",
    nativeGraphNoTVCR,
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
      {
        id: "MinCut",
        icon: <Scissors />,
        tooltip: "Min Cut",
        render: () => <MinCutPanel />,
      },
      {
        id: "DebugPanel",
        icon: <Bug />,
        tooltip: "Debug",
        render: () => <DebugPanel />,
      },
      {
        id: "ExportGraph",
        icon: <Download />,
        tooltip: "Export Graph",
        render: () => <ExportGraphPanel />,
      },
    ],
    [],
  );

  const resolvedPanels = useMemo(() => {
    const hiddenSet = new Set(hidden_panels ?? []);
    // Min cut operates on a single index space, so it's meaningless when
    // comparing two graphs — hide it whenever a left graph is present.
    const isCompareMode = left.nativeGraph != null;
    const builtins = BUILTIN_PANELS.filter(
      (p) =>
        !hiddenSet.has(p.id as BuiltinSidebarPanel) &&
        !(isCompareMode && p.id === "MinCut"),
    );
    return [...builtins, ...(customPanels ?? [])];
  }, [BUILTIN_PANELS, customPanels, hidden_panels, left.nativeGraph]);

  return (
    <div className="h-screen flex flex-col unigraph-explorer bg-background">
      <DebugModeContextProvider>
        <GlobalElementRefsContextProvider>
          <ErrorBoundary>
            <ResolvedSourceContextProvider source={resolvedSource}>
              <PluginsContextProvider plugins={plugins}>
                <NativeGraphContextProvider
                  nativeGraphL={left.nativeGraph}
                  nativeGraphR={right.nativeGraph!}
                >
                  <TraversalConfigContextProvider
                    tvcL={left.tvc}
                    setTvcL={left.setTvc}
                    tvcR={right.tvc!}
                    setTvcR={right.setTvc}
                  >
                    <SimulationParamsContextProvider>
                      <SelectedNodesContextProvider>
                        <MetricViewStateProvider
                          nativeGraph={right.nativeGraph!}
                        >
                          <SelectedPathContextProvider syncToURL={true}>
                            <MinCutContextProvider>
                              <Page
                                homeHref={home_href}
                                panels={resolvedPanels}
                                source={source}
                              />
                            </MinCutContextProvider>
                          </SelectedPathContextProvider>
                        </MetricViewStateProvider>
                      </SelectedNodesContextProvider>
                    </SimulationParamsContextProvider>
                  </TraversalConfigContextProvider>
                </NativeGraphContextProvider>
              </PluginsContextProvider>
            </ResolvedSourceContextProvider>
          </ErrorBoundary>
        </GlobalElementRefsContextProvider>
      </DebugModeContextProvider>
    </div>
  );
}

function Page({
  homeHref,
  panels,
  source,
}: {
  homeHref?: string;
  panels: ResolvedPanel[];
  source: ExplorerGraphSource;
}) {
  const [nativeGraphL, nativeGraphR] = useNativeGraphs();
  const [settings] = useGraphSettings();
  const [selectedNodes] = useSelectedNodes();
  const portalRef = usePortalContainer();
  const [debugMode] = useDebugMode();
  useGlobalKeyboardShortcuts();

  const visiblePanels = useMemo(
    () => (debugMode ? panels : panels.filter((p) => p.id !== "DebugPanel")),
    [panels, debugMode],
  );

  const selectedSidebarPanel =
    settings.ui_settings?.selected_sidebar_panel ?? "None";

  const panelTab =
    visiblePanels.find((p) => p.id === selectedSidebarPanel)?.render() ?? null;

  const entryPointsConfig: EntryPointsConfig = useMemo(
    () => ({
      entryPoints: settings.ui_settings?.entry_points ?? "Determine",
      specified: settings.ui_settings?.entry_points_specified ?? null,
      filter: settings.ui_settings?.entry_points_filter ?? null,
    }),
    [
      settings.ui_settings?.entry_points,
      settings.ui_settings?.entry_points_specified,
      settings.ui_settings?.entry_points_filter,
    ],
  );

  const roots = useMemo(() => {
    const rootsR = getRoots(nativeGraphR, selectedNodes, entryPointsConfig);

    if (nativeGraphL == null) {
      return rootsR;
    } else {
      const rootsL = getRoots(nativeGraphL, selectedNodes, entryPointsConfig);
      return Array.from(new Set([...rootsL, ...rootsR]));
    }
  }, [nativeGraphL, nativeGraphR, selectedNodes, entryPointsConfig]);

  return (
    <div
      className="flex grow-1 shrink flex-row bg-background text-foreground min-h-0"
      ref={portalRef}
    >
      <Sidebar
        selectedPanelTab={selectedSidebarPanel}
        homeHref={homeHref}
        panels={visiblePanels}
        source={source}
      />
      {panelTab}
      <div className="flex flex-col h-full grow-1 min-w-0">
        <GraphTreeTable focusOnMount={true} roots={roots} />
        <ExplorerFooter />
      </div>
    </div>
  );
}

type EntryPointsConfig = {
  entryPoints: ArrayGraphUISettingsTreeTableEntryPoints;
  specified: string[] | null;
  filter: NodeSelection | null;
};

function getRoots(
  nativeGraph: NativeGraph,
  selectedNodes: NodeIDX[],
  { entryPoints, specified, filter }: EntryPointsConfig,
): readonly NodeIDX[] {
  if (selectedNodes.length > 0) {
    return selectedNodes;
  }

  switch (entryPoints) {
    case "Determine":
      return nativeGraph.determineEntrypoints().vec;
    case "AllReachable":
      return nativeGraph.getAllReachableNodeIDXs().vec;
    case "Filtered":
      if (filter == null) {
        return nativeGraph.getAllReachableNodeIDXs().vec;
      }
      return nativeGraph.filteredEntrypoints(filter).vec;
    case "Specified":
      if (specified == null || specified.length === 0) {
        return nativeGraph.determineEntrypoints().vec;
      }

      return specified
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
  deltaParamKey: string,
  nativeGraphNoTVC: NativeGraph | null,
) {
  const { params, setParam } = useSearchParamsContext();
  const gqcDelta = params[deltaParamKey];

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
    const inlineTvc =
      currentGqc.traversal != null && "Inline" in currentGqc.traversal
        ? (currentGqc.traversal.Inline as TraversalConfig)
        : null;
    const tvc: TraversalConfig =
      inlineTvc ?? nativeGraphNoTVC.getTraversalConfig();
    return [tvc, nativeGraphNoTVC.getApplyTraversalConfig(tvc)];
  }, [resolvedBase, gqcDelta, nativeGraphNoTVC]);

  const setTvc = useCallback(
    (tvc: TraversalConfig) => {
      if (resolvedBase == null) {
        return;
      }
      const modified: GraphQueryConfig = {
        ...resolvedBase,
        traversal: { Inline: tvc },
      };
      const delta = derive_gqc_delta(
        JSON.stringify(resolvedBase),
        JSON.stringify(modified),
      );
      setParam(deltaParamKey, delta);
    },
    [resolvedBase, setParam, deltaParamKey],
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
    handle: "local",
    traversal: { Inline: nativeGraph.getTraversalConfig() },
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
    getGraphData(graphs.right) !== getGraphData(ref.current.right) ||
    getGraphData(graphs.left) !== getGraphData(ref.current.left)
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

// ---------------------------------------------------------------------------
// Source-based data fetching (used by ExplorerFetcher)
// ---------------------------------------------------------------------------

interface FetchResult {
  graphs: ExplorerComponentInputGraphs;
  resolved: ResolvedGraphSource;
  baseGqcL: GraphQueryConfig | null;
  baseGqcR: GraphQueryConfig | null;
}

async function fetchGraphSource(
  rpc: UnigraphRpc,
  source: ExplorerGraphSource,
): Promise<FetchResult> {
  return fetchHandleGraphs(rpc, source.right, source.left);
}

function graphQueryOutputToInputGraph(
  output: GraphQueryOutput,
): ExplorerComponentInputGraph {
  return {
    ArrayGraphSerializedPackageBase64: {
      data: JSON.stringify(output.package),
      format: "Json",
    },
  };
}

async function fetchHandleGraph(
  rpc: UnigraphRpc,
  gh: ExplorerGraphHandle,
): Promise<GraphQueryOutput> {
  return rpc.call("GraphQuery", {
    query: {
      handle: gh.handle,
      roots: gh.roots,
      traversal: gh.traversal,
    },
  });
}

async function fetchHandleGraphs(
  rpc: UnigraphRpc,
  right: ExplorerGraphHandle,
  left: ExplorerGraphHandle | undefined,
): Promise<FetchResult> {
  const rightPromise = fetchHandleGraph(rpc, right);
  const leftPromise =
    left != null ? fetchHandleGraph(rpc, left) : Promise.resolve(null);

  const [rightResult, leftResult] = await Promise.all([
    rightPromise,
    leftPromise,
  ]);

  return {
    graphs: {
      right: graphQueryOutputToInputGraph(rightResult),
      left:
        leftResult != null
          ? graphQueryOutputToInputGraph(leftResult)
          : undefined,
    },
    resolved: {
      right: resolveGraphRef(right, rightResult),
      left:
        left != null && leftResult != null
          ? resolveGraphRef(left, leftResult)
          : undefined,
    },
    baseGqcL: leftResult?.graph_query_config ?? null,
    baseGqcR: rightResult.graph_query_config,
  };
}

/**
 * Pair the handle we asked for with the concrete snapshot the server landed on.
 * `output.graph_key` is authoritative — `gh.handle` may be an anonymous
 * `gqc_…` key or a bare timeline meaning "latest".
 */
function resolveGraphRef(
  gh: ExplorerGraphHandle,
  output: GraphQueryOutput,
): ResolvedGraphRef {
  return {
    handle: gh.handle,
    graph_key: output.graph_key,
    ...parseGraphKey(output.graph_key),
  };
}

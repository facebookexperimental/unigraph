// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo, useRef, useState } from "react";
import type { TraversalConfig } from "u-be/unigraph_core/bindings/TraversalConfig";
import {
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import type { GraphSettings } from "../u-be/unigraph_core/bindings/GraphSettings";
import ExplorerFooter from "./ExplorerFooter";
import NativeGraph from "./NativeGraph";
import Sidebar from "./Sidebar";
import Simulation from "./Simulation";
import { PortalContextProvider } from "./components/PortalContext";
import {
  GraphSettingsContextProvider,
  useGraphSettings,
} from "./context/GraphSettingsContext";
import {
  NativeGraphContextProvider,
  useNativeGraph,
} from "./context/NativeGraphContext";
import { TraversalConfigContextProvider } from "./context/TraversalConfigContext";
import initWasm from "./init_wasm";
import ColumnsPanel from "./sidebar_panels/ColumnsPanel";
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
    <NativeGraphContextProvider nativeGraph={nativeGraph}>
      <TraversalConfigContextProvider tvc={tvc} setTvc={setTvcCb}>
        <GraphSettingsContextProvider
          settings={settings}
          setSettings={setSettingsCb}
        >
          <div className="h-screen flex flex-col unigraph-explorer">
            <Page />
          </div>
        </GraphSettingsContextProvider>
      </TraversalConfigContextProvider>
    </NativeGraphContextProvider>
  );
}

function Page() {
  const [selectedNodeIDXs, setSelectedNodeIDXs] = useState<NodeIDX[]>([]);
  const nativeGraph = useNativeGraph();
  const [graphSettings] = useGraphSettings();
  const [settings] = useGraphSettings();

  const containerRef = useRef<HTMLDivElement>(null);

  const setSelectedNodeIDXsCb = useCallback(
    (idxs: NodeIDX[]) => {
      // if updating from an emmpty array to empty array we don't
      // want to trigger a rerender
      if (idxs.length === 0 && selectedNodeIDXs.length === 0) {
        return;
      }

      setSelectedNodeIDXs(idxs);
    },
    [selectedNodeIDXs],
  );

  const selectedSidebarPanel =
    settings.ui_settings?.selected_sidebar_panel ?? "Simulation";

  const panelTab: React.ReactNode = (() => {
    switch (selectedSidebarPanel) {
      case "Simulation":
        return <Simulation setSelectedNodeIDXs={setSelectedNodeIDXsCb} />;
      case "None":
        return null;
      case "GraphInfo":
        return <GraphInfoPanel />;
      case "ColumnsSettings":
        return <ColumnsPanel />;
      default: {
        const exhaustiveCheck: never = selectedSidebarPanel;
        throw new Error(`Unexpected panel tab: ${exhaustiveCheck}`);
      }
    }
  })();

  const roots = useMemo(() => {
    if (selectedNodeIDXs.length > 0) {
      return selectedNodeIDXs;
    }
    if (graphSettings.ui_settings?.show_as_a_flat_list) {
      return nativeGraph.getAllReachableNodeIDXs().vec;
    } else {
      return nativeGraph.determineEntrypoints().vec;
    }
  }, [
    nativeGraph,
    selectedNodeIDXs,
    graphSettings.ui_settings?.show_as_a_flat_list,
  ]);

  return (
    <PortalContextProvider containerRef={containerRef}>
      <div
        className="flex grow-1 shrink flex-row bg-background text-foreground min-h-0"
        ref={containerRef}
      >
        <Sidebar selectedPanelTab={selectedSidebarPanel} />
        {panelTab}
        <div className="flex flex-col h-full grow-1">
          <GraphTreeTable focusOnMount={true} roots={roots} />
          <ExplorerFooter selectedNodeIDXs={selectedNodeIDXs} />
        </div>
      </div>
    </PortalContextProvider>
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

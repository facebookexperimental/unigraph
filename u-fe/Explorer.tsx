// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo, useRef, useState } from "react";
import type { TraversalConfig } from "u-be/unigraph_core/bindings/TraversalConfig";
import { TraversalConfigContextProvider } from "./context/TraversalConfigContext";
import {
  type PageParams,
  PageParamsProvider,
  usePageParams,
} from "./PageParams";
import NativeGraph from "./NativeGraph";
import type { NodeIDX } from "./types";
import Simulation from "./Simulation";
import GraphTreeTable from "./tree_table/GraphTreeTable";
import Sidebar from "./Sidebar";
import initWasm from "./init_wasm";
import GraphInfoPanel from "./sidebar_panels/GraphInfoPanel";
import { PortalContextProvider } from "./components/PortalContext";
import ColumnsPanel from "./sidebar_panels/ColumnsPanel";
import { GraphTreeTableColumnsContextProvider } from "./context/GraphTreeTableColumnsContext";
import {
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../.build/wasm/unigraph_wasm";
import {
  NativeGraphContextProvider,
  useNativeGraph,
} from "./context/NativeGraphContext";

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
  onPageParamsChange,
  pageParams = {},
  graph,
  header,
  traversalConfigZSTDBase64UrlSafeNoPadding,
  onTraversalConfigZSTDBase64UrlSafeNoPaddingChange,
}: {
  onPageParamsChange?: (params: PageParams) => void;
  pageParams?: PageParams;
  graph: InputGraph;
  header?: React.ReactNode;
  traversalConfigZSTDBase64UrlSafeNoPadding?: string | null;
  onTraversalConfigZSTDBase64UrlSafeNoPaddingChange?: (v: string) => void;
}) {
  initWasm();

  /// This graph initializes a new native graph every time the raw data changes.
  const nativeGraphNoTVC = useMemo(() => initNativeGraph(graph), [graph]);

  /// This hook will NOT re-initialize the native graph if the traversal config changes.
  /// We modify it in place and return a new nativeGraph reference with all caches busted.
  const [tvc, nativeGraph] = useMemo(() => {
    const tvc: TraversalConfig =
      traversalConfigZSTDBase64UrlSafeNoPadding == null
        ? { ...DEFAULT_TVC }
        : JSON.parse(
            from_zstd_base64_url_safe_no_pad(
              traversalConfigZSTDBase64UrlSafeNoPadding,
            ),
          );

    return [tvc, nativeGraphNoTVC.getApplyTraversalConfig(tvc)];
  }, [traversalConfigZSTDBase64UrlSafeNoPadding, nativeGraphNoTVC]);

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

  return (
    <NativeGraphContextProvider nativeGraph={nativeGraph}>
      <TraversalConfigContextProvider tvc={tvc} setTvc={setTvcCb}>
        <PageParamsProvider
          onPageParamsChange={onPageParamsChange}
          initialParams={pageParams}
        >
          <div className="h-screen flex flex-col">
            {header}
            <Page />
          </div>
        </PageParamsProvider>
      </TraversalConfigContextProvider>
    </NativeGraphContextProvider>
  );
}

function Page() {
  const [selectedNodeIDXs, setSelectedNodeIDXs] = useState<NodeIDX[]>([]);
  const nativeGraph = useNativeGraph();

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

  const [pageParams] = usePageParams();
  const selectedPanelTab = pageParams.panelTab ?? "Simulation";

  const panelTab: React.ReactNode = (() => {
    switch (selectedPanelTab) {
      case "Simulation":
        return <Simulation setSelectedNodeIDXs={setSelectedNodeIDXsCb} />;
      case "None":
        return null;
      case "GraphInfo":
        return <GraphInfoPanel />;
      case "Columns":
        return <ColumnsPanel />;
      default: {
        const exhaustiveCheck: never = selectedPanelTab;
        throw new Error(`Unexpected panel tab: ${exhaustiveCheck}`);
      }
    }
  })();

  const roots =
    selectedNodeIDXs.length > 0
      ? selectedNodeIDXs
      : nativeGraph.determineEntrypoints();

  return (
    <PortalContextProvider containerRef={containerRef?.current}>
      <GraphTreeTableColumnsContextProvider>
        <div
          className="flex grow-1 shrink flex-row bg-background text-foreground unigraph-explorer min-h-0"
          ref={containerRef}
        >
          <Sidebar selectedPanelTab={selectedPanelTab} />
          {panelTab}
          <div className="flex h-full grow-1">
            <GraphTreeTable focusOnMount={true} roots={roots} />
          </div>
        </div>
      </GraphTreeTableColumnsContextProvider>
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

const DEFAULT_TVC: TraversalConfig = {
  force_nodes: {},
  force_edges: {},
  force_dynamic: [],
  tag_sets: [],
  tiered_traversal: {
    AscendingTiers: {
      tiers: [
        {
          name: "T1",
          tags_that_transition_to_this_tier: [],
        },
        {
          name: "T2",
          tags_that_transition_to_this_tier: ["RDFD"],
        },
        {
          name: "T3",
          tags_that_transition_to_this_tier: ["RD"],
        },
        {
          name: "T4",
          tags_that_transition_to_this_tier: ["BL"],
        },
      ],
    },
  },
};

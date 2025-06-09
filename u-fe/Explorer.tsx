// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useRef, useState } from "react";
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
  initialPageParams = {},
  graph,
  header,
}: {
  onPageParamsChange?: (params: PageParams) => void;
  initialPageParams?: PageParams;
  graph: InputGraph;
  header?: React.ReactNode;
}) {
  initWasm();

  const [tvc, setTvc] = useState<TraversalConfig>({ ...DEFAULT_TVC });

  const [nativeGraph, setNativeGraph] = useState<NativeGraph>(() =>
    initNativeGraph(graph).getApplyTraversalConfig(tvc),
  );

  const setTvcCb = useCallback(
    (tvc: TraversalConfig) => {
      setTvc(tvc);
      setNativeGraph(nativeGraph.getApplyTraversalConfig(tvc));
    },
    [nativeGraph],
  );

  useEffect(() => {
    setTvc({ ...DEFAULT_TVC });
    setNativeGraph(
      initNativeGraph(graph).getApplyTraversalConfig({ ...DEFAULT_TVC }),
    );
  }, [graph]);

  return (
    <TraversalConfigContextProvider tvc={tvc} setTvc={setTvcCb}>
      <PageParamsProvider
        onPageParamsChange={onPageParamsChange}
        initialParams={initialPageParams}
      >
        <div className="h-screen flex flex-col">
          {header}
          <Page nativeGraph={nativeGraph} />
        </div>
      </PageParamsProvider>
    </TraversalConfigContextProvider>
  );
}

function Page({ nativeGraph }: { nativeGraph: NativeGraph }) {
  const [selectedNodeIDXs, setSelectedNodeIDXs] = useState<NodeIDX[]>([]);

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
        return <GraphInfoPanel nativeGraph={nativeGraph} />;
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
      <GraphTreeTableColumnsContextProvider nativeGraph={nativeGraph}>
        <div
          className="flex grow-1 shrink flex-row bg-background text-foreground unigraph-explorer min-h-0"
          ref={containerRef}
        >
          <Sidebar selectedPanelTab={selectedPanelTab} />
          {panelTab}
          <div className="flex h-full grow-1">
            <GraphTreeTable
              focusOnMount={true}
              roots={roots}
              nativeGraph={nativeGraph}
            />
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

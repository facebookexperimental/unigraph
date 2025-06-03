// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useState } from "react";
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

export type InputGraph =
  | {
      t: "MapGraphJSON";
      mapGraphJSON: string;
    }
  | {
      t: "ArrayGraphSerializable_json_zstd_base64";
      arrayGraphSerializable_json_zstd_base64: string;
    };

export function Explorer({
  onPageParamsChange,
  initialPageParams = {},
  graph,
}: {
  onPageParamsChange?: (params: PageParams) => void;
  initialPageParams?: PageParams;
  graph: InputGraph;
}) {
  return (
    <ExplorerImpl
      onPageParamsChange={onPageParamsChange}
      initialPageParams={initialPageParams}
      graph={graph}
    />
  );
}

function ExplorerImpl({
  onPageParamsChange,
  initialPageParams = {},
  graph,
}: {
  onPageParamsChange?: (params: PageParams) => void;
  initialPageParams?: PageParams;
  graph: InputGraph;
}) {
  initWasm();

  const [nativeGraph, setNativeGraph] = useState<NativeGraph>(() =>
    initNativeGraph(graph),
  );

  const [tvc, setTvc] = useState<TraversalConfig>({
    force_nodes: {},
    force_edges: {},
    force_dynamic: [],
    tag_sets: [],
    tiered_traversal: null,
  });

  const setTvcCb = useCallback(
    (tvc: TraversalConfig) => {
      setTvc(tvc);
      setNativeGraph(nativeGraph.getApplyTraversalConfig(tvc));
    },
    [nativeGraph],
  );

  useEffect(() => {
    setNativeGraph(initNativeGraph(graph));
  }, [graph]);

  return (
    <TraversalConfigContextProvider tvc={tvc} setTvc={setTvcCb}>
      <PageParamsProvider
        onPageParamsChange={onPageParamsChange}
        initialParams={initialPageParams}
      >
        <Page nativeGraph={nativeGraph} />
      </PageParamsProvider>
    </TraversalConfigContextProvider>
  );
}

function Page({ nativeGraph }: { nativeGraph: NativeGraph }) {
  const [selectedNodeIDXs, setSelectedNodeIDXs] = useState<NodeIDX[]>([]);

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
    <div className="flex h-screen flex-row bg-background text-foreground unigraph-explorer">
      <Sidebar selectedPanelTab={selectedPanelTab} />
      {panelTab}
      <div className="flex grow-1">
        <GraphTreeTable
          focusOnMount={true}
          roots={roots}
          nativeGraph={nativeGraph}
        />
      </div>
    </div>
  );
}

function initNativeGraph(graph: InputGraph): NativeGraph {
  switch (graph.t) {
    case "ArrayGraphSerializable_json_zstd_base64":
      throw new Error("not implemented yet");
    case "MapGraphJSON":
      return NativeGraph.fromMapGraphJSON(graph.mapGraphJSON);
  }
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Single source of truth for graph settings + metric view resolution.
//!
//! State lifecycle:
//!   1. Init: search param → parse → push to WASM. No param → read from WASM.
//!   2. User changes settings: push to WASM → read visible/available back → sync param.
//!   3. Graph structure or traversal config changes: re-resolve from WASM.
//!
//! WASM is the source of truth. React state mirrors it.
//! Search param `graph_settings` (zstd+base64) is kept in sync automatically.

import { createContext, useCallback, useContext, useMemo } from "react";
import type { GraphSettings } from "@/__generated__/ts/GraphSettings";
import type NativeGraph from "../native/NativeGraph";
import {
  from_zstd_base64_url_safe_no_pad,
  to_zstd_base64_url_safe_no_pad,
} from "../../.build/wasm/unigraph_wasm";
import { useSearchParamsContext } from "./SearchParamsContext";

const GRAPH_SETTINGS_PARAM = "graph_settings";

export interface MetricViewState {
  graphSettings: GraphSettings;
  availableViews: string[];
  visibleViews: Set<string>;
  setGraphSettings: (gs: GraphSettings) => void;
}

const MetricViewStateContext = createContext<MetricViewState | null>(null);

export function MetricViewStateProvider({
  children,
  nativeGraph,
}: {
  children: React.ReactNode;
  nativeGraph: NativeGraph;
}) {
  const { params, setParam } = useSearchParamsContext();

  const graphSettings = useMemo(() => {
    const param = params[GRAPH_SETTINGS_PARAM];
    if (param != null) {
      const gs: GraphSettings = JSON.parse(
        from_zstd_base64_url_safe_no_pad(param),
      );
      nativeGraph.setGraphSettings(gs);
      return gs;
    }
    return nativeGraph.getGraphSettings();
  }, [params, nativeGraph]);

  const graphStructure =
    graphSettings.ui_settings?.graph_structure ?? "Forward";
  const structureNum = graphStructureToNum(graphStructure);

  const availableViews = useMemo(
    () => nativeGraph.getAvailableMetricViews(),
    [nativeGraph, graphSettings.metrics_config],
  );

  const visibleViews = useMemo(
    () => new Set(nativeGraph.getVisibleMetricViews(structureNum)),
    [nativeGraph, graphSettings, structureNum],
  );

  const setGraphSettings = useCallback(
    (gs: GraphSettings) => {
      nativeGraph.setGraphSettings(gs);
      const base64 = to_zstd_base64_url_safe_no_pad(JSON.stringify(gs));
      setParam(GRAPH_SETTINGS_PARAM, base64);
    },
    [nativeGraph, setParam],
  );

  const value = useMemo(
    () => ({
      graphSettings,
      availableViews,
      visibleViews,
      setGraphSettings,
    }),
    [graphSettings, availableViews, visibleViews, setGraphSettings],
  );

  return (
    <MetricViewStateContext.Provider value={value}>
      {children}
    </MetricViewStateContext.Provider>
  );
}

export function useMetricViewState(): MetricViewState {
  const context = useContext(MetricViewStateContext);
  if (context == null) {
    throw new Error(
      "useMetricViewState must be used within a MetricViewStateProvider",
    );
  }
  return context;
}

function graphStructureToNum(structure: string): number {
  switch (structure) {
    case "Dominator":
      return 1;
    case "Reverse":
      return 2;
    default:
      return 0;
  }
}

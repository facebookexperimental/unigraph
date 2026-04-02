// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import type {
  ExplorerComponentInputGraph,
  ExplorerComponentInputGraphs,
} from "../../Explorer";
import type { GraphQueryConfig } from "../../__generated__/ts/GraphQueryConfig";
import type { GraphQueryOutput } from "../../__generated__/ts/GraphQueryOutput";
import { useRpc, type UnigraphRpc } from "../../api/rpc";
import { Explorer } from "../../Explorer";

const QUERY_PARAM_GQC_DELTA_L = "gqc_deltaL";
const QUERY_PARAM_GQC_DELTA_R = "gqc_deltaR";
const QUERY_PARAM_GRAPH_SETTINGS = "graph_settings";

interface LocalGraphsApiResponse {
  left?: ExplorerComponentInputGraph;
  right: ExplorerComponentInputGraph;
}

export default function ExplorerRoute() {
  const { handleL, handleR } = useParams();
  const isLocal = handleL == null;
  const rpc = useRpc();
  const [searchParams, setSearchParams] = useSearchParams();

  const [graphs, setGraphs] = useState<ExplorerComponentInputGraphs | null>(
    null,
  );
  const [baseGqcL, setBaseGqcL] = useState<GraphQueryConfig | null>(null);
  const [baseGqcR, setBaseGqcR] = useState<GraphQueryConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isLocal) {
      fetchLocalGraphs().then(setGraphs).catch(handleError);
    } else {
      fetchHandleGraphs(rpc, handleL, handleR)
        .then(applyHandleResults)
        .catch(handleError);
    }

    function handleError(e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }

    function applyHandleResults(results: HandleFetchResults) {
      setGraphs({ right: results.rightGraph, left: results.leftGraph });
      setBaseGqcL(results.baseGqcL);
      setBaseGqcR(results.baseGqcR);
    }
  }, [isLocal, handleL, handleR, rpc]);

  const gqcDeltaL = searchParams.get(QUERY_PARAM_GQC_DELTA_L) ?? undefined;
  const gqcDeltaR = searchParams.get(QUERY_PARAM_GQC_DELTA_R) ?? undefined;
  const graphSettings =
    searchParams.get(QUERY_PARAM_GRAPH_SETTINGS) ?? undefined;

  const baseGqcLJson = useMemo(
    () => (baseGqcL != null ? JSON.stringify(baseGqcL) : undefined),
    [baseGqcL],
  );
  const baseGqcRJson = useMemo(
    () => (baseGqcR != null ? JSON.stringify(baseGqcR) : undefined),
    [baseGqcR],
  );

  const onGqcDeltaChangeL = useCallback(
    (value: string) => {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        if (value === "") {
          next.delete(QUERY_PARAM_GQC_DELTA_L);
        } else {
          next.set(QUERY_PARAM_GQC_DELTA_L, value);
        }
        return next;
      });
    },
    [setSearchParams],
  );

  const onGqcDeltaChangeR = useCallback(
    (value: string) => {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        if (value === "") {
          next.delete(QUERY_PARAM_GQC_DELTA_R);
        } else {
          next.set(QUERY_PARAM_GQC_DELTA_R, value);
        }
        return next;
      });
    },
    [setSearchParams],
  );

  const onGraphSettingsChange = useCallback(
    (value: string) => {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.set(QUERY_PARAM_GRAPH_SETTINGS, value);
        return next;
      });
    },
    [setSearchParams],
  );

  if (error != null) {
    return (
      <div className="p-4 text-red-500">Failed to load graph: {error}</div>
    );
  }

  if (graphs == null) {
    return (
      <div className="h-screen flex items-center justify-center">Loading…</div>
    );
  }

  return (
    <Explorer
      graphs={graphs}
      config={{
        base_gqc_l: baseGqcLJson,
        base_gqc_r: baseGqcRJson,
        gqc_delta_l: gqcDeltaL,
        on_gqc_delta_change_l: onGqcDeltaChangeL,
        gqc_delta_r: gqcDeltaR,
        on_gqc_delta_change_r: onGqcDeltaChangeR,
        graph_settings: graphSettings,
        on_graph_settings_change: onGraphSettingsChange,
      }}
      home_href={isLocal ? undefined : "/"}
    />
  );
}

// --- Data fetching helpers ---

async function fetchLocalGraphs(): Promise<ExplorerComponentInputGraphs> {
  const r = await fetch("/api/local_graphs");
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  const data: LocalGraphsApiResponse = await r.json();
  return { right: data.right, left: data.left };
}

interface HandleFetchResults {
  rightGraph: ExplorerComponentInputGraph;
  leftGraph?: ExplorerComponentInputGraph;
  baseGqcL: GraphQueryConfig;
  baseGqcR: GraphQueryConfig | null;
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
  handle: string,
): Promise<GraphQueryOutput> {
  if (handle.startsWith("gqc-")) {
    return rpc.call("GraphQuery", { graph_query_config_key: handle });
  }
  return rpc.call("GraphQuery", {
    graph_query_config: { roots: [], handle },
  });
}

async function fetchHandleGraphs(
  rpc: UnigraphRpc,
  handleL: string,
  handleR: string | undefined,
): Promise<HandleFetchResults> {
  const leftPromise = fetchHandleGraph(rpc, handleL);
  const rightPromise =
    handleR != null ? fetchHandleGraph(rpc, handleR) : Promise.resolve(null);

  const [leftResult, rightResult] = await Promise.all([
    leftPromise,
    rightPromise,
  ]);

  return {
    rightGraph: graphQueryOutputToInputGraph(leftResult),
    leftGraph:
      rightResult != null
        ? graphQueryOutputToInputGraph(rightResult)
        : undefined,
    baseGqcL: leftResult.graph_query_config,
    baseGqcR: rightResult?.graph_query_config ?? null,
  };
}

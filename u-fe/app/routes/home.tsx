// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import type { ExplorerComponentInputGraph } from "../../__generated__/ts/ExplorerComponentInputGraph";
import type { ExplorerComponentInputGraphs } from "../../__generated__/ts/ExplorerComponentInputGraphs";
import { Explorer } from "../../Explorer";

const QUERY_PARAM_TVC_L = "tvc";
const QUERY_PARAM_TVC_R = "tvc_r";
const QUERY_PARAM_GRAPH_SETTINGS = "graph_settings";

interface GraphsApiResponse {
  left: ExplorerComponentInputGraph;
  right?: ExplorerComponentInputGraph;
}

export default function Home() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [graphs, setGraphs] = useState<ExplorerComponentInputGraphs | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/graphs")
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((data: GraphsApiResponse) => {
        setGraphs({ left: data.left, right: data.right });
      })
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, []);

  const tvcL = searchParams.get(QUERY_PARAM_TVC_L) ?? undefined;
  const tvcR = searchParams.get(QUERY_PARAM_TVC_R) ?? undefined;
  const graphSettings =
    searchParams.get(QUERY_PARAM_GRAPH_SETTINGS) ?? undefined;

  const onTvcChangeL = useCallback(
    (value: string) => {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.set(QUERY_PARAM_TVC_L, value);
        return next;
      });
    },
    [setSearchParams],
  );

  const onTvcChangeR = useCallback(
    (value: string) => {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.set(QUERY_PARAM_TVC_R, value);
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
      traversal_config_l={tvcL}
      on_traversal_config_change_l={onTvcChangeL}
      traversal_config_r={tvcR}
      on_traversal_config_change_r={onTvcChangeR}
      graph_settings={graphSettings}
      on_graph_settings_change={onGraphSettingsChange}
    />
  );
}

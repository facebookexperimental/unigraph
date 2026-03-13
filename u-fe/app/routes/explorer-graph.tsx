// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import type { ExplorerComponentInputGraph } from "../../__generated__/ts/ExplorerComponentInputGraph";
import type { ExplorerComponentInputGraphs } from "../../__generated__/ts/ExplorerComponentInputGraphs";
import { Explorer } from "../../Explorer";

const QUERY_PARAM_TVC_L = "tvc";
const QUERY_PARAM_TVC_R = "tvc_r";
const QUERY_PARAM_GRAPH_SETTINGS = "graph_settings";

const QUERY_PARAM_RIGHT = "right";

interface GraphsApiResponse {
  left: ExplorerComponentInputGraph;
  right?: ExplorerComponentInputGraph;
}

async function fetchGraph(
  timelineId: string,
  graphId: string,
): Promise<ExplorerComponentInputGraph> {
  const r = await fetch(`/api/timelines/${timelineId}/graphs/${graphId}`);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  const data: GraphsApiResponse = await r.json();
  return data.left;
}

export default function ExplorerGraphRoute() {
  const { timelineId, graphId } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const rightGraphId = searchParams.get(QUERY_PARAM_RIGHT);
  const [graphs, setGraphs] = useState<ExplorerComponentInputGraphs | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (timelineId == null || graphId == null) return;

    const leftPromise = fetchGraph(timelineId, graphId);
    const rightPromise =
      rightGraphId != null
        ? fetchGraph(timelineId, rightGraphId)
        : Promise.resolve(undefined);

    Promise.all([leftPromise, rightPromise])
      .then(([left, right]) => {
        setGraphs({ left, right });
      })
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, [timelineId, graphId, rightGraphId]);

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
      <div className="h-screen flex items-center justify-center">
        Loading...
      </div>
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

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback } from "react";
import { useParams, useSearchParams } from "react-router";
import ErrorBoundary from "../../components/ErrorBoundary";
import { Explorer, type ExplorerGraphSource } from "../../Explorer";

const QUERY_PARAM_GQC_DELTA_L = "gqc_deltaL";
const QUERY_PARAM_GQC_DELTA_R = "gqc_deltaR";
const QUERY_PARAM_GRAPH_SETTINGS = "graph_settings";

export default function ExplorerRoute() {
  const { handleR, handleL } = useParams();
  const isLocal = handleR == null;
  const [searchParams, setSearchParams] = useSearchParams();

  const source: ExplorerGraphSource = isLocal
    ? { type: "local" }
    : {
        type: "handle",
        right: { handle: handleR },
        left: handleL != null ? { handle: handleL } : undefined,
      };

  const gqcDeltaL = searchParams.get(QUERY_PARAM_GQC_DELTA_L) ?? undefined;
  const gqcDeltaR = searchParams.get(QUERY_PARAM_GQC_DELTA_R) ?? undefined;
  const graphSettings =
    searchParams.get(QUERY_PARAM_GRAPH_SETTINGS) ?? undefined;

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

  return (
    <ErrorBoundary>
      <Explorer
        source={source}
        config={{
          gqc_delta_l: gqcDeltaL,
          on_gqc_delta_change_l: onGqcDeltaChangeL,
          gqc_delta_r: gqcDeltaR,
          on_gqc_delta_change_r: onGqcDeltaChangeR,
          graph_settings: graphSettings,
          on_graph_settings_change: onGraphSettingsChange,
        }}
        home_href={isLocal ? undefined : "/"}
      />
    </ErrorBoundary>
  );
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import { useParams, useSearchParams } from "react-router";
import ErrorBoundary from "../../components/ErrorBoundary";
import { Explorer, type ExplorerGraphSource } from "../../Explorer";

const QUERY_PARAM_GQC_DELTA_L = "gqc_deltaL";
const QUERY_PARAM_GQC_DELTA_R = "gqc_deltaR";

const LOCAL_SOURCE: ExplorerGraphSource = { type: "local" };

export default function ExplorerRoute() {
  const { handleR, handleL } = useParams();
  const isLocal = handleR == null;

  const source: ExplorerGraphSource = useMemo(
    () =>
      isLocal
        ? LOCAL_SOURCE
        : {
            type: "handle",
            right: { handle: handleR },
            left: handleL != null ? { handle: handleL } : undefined,
          },
    [isLocal, handleR, handleL],
  );

  return (
    <ExplorerWithSearchParams
      source={source}
      home_href={isLocal ? undefined : "/"}
    />
  );
}

export function LocalExplorerRoute() {
  return <ExplorerWithSearchParams source={LOCAL_SOURCE} />;
}

function ExplorerWithSearchParams({
  source,
  home_href,
}: {
  source: ExplorerGraphSource;
  home_href?: string;
}) {
  const [searchParams, setSearchParams] = useSearchParams();

  const gqcDeltaL = searchParams.get(QUERY_PARAM_GQC_DELTA_L) ?? undefined;
  const gqcDeltaR = searchParams.get(QUERY_PARAM_GQC_DELTA_R) ?? undefined;

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

  return (
    <ErrorBoundary>
      <Explorer
        source={source}
        config={{
          gqc_delta_l: gqcDeltaL,
          on_gqc_delta_change_l: onGqcDeltaChangeL,
          gqc_delta_r: gqcDeltaR,
          on_gqc_delta_change_r: onGqcDeltaChangeR,
        }}
        home_href={home_href}
      />
    </ErrorBoundary>
  );
}

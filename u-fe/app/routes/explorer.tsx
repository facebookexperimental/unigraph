// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import { useParams, useSearchParams } from "react-router";
import ErrorBoundary from "../../components/ErrorBoundary";
import { Explorer, type ExplorerGraphSource } from "../../Explorer";

export default function ExplorerRoute() {
  const { handleR, handleL } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();

  const source: ExplorerGraphSource = useMemo(
    () => ({
      type: "handle",
      // `handleR` is always present — both routes bind it.
      right: { handle: handleR ?? "" },
      left: handleL != null ? { handle: handleL } : undefined,
    }),
    [handleR, handleL],
  );

  const initialSearchParams = useMemo(
    () => Object.fromEntries(searchParams.entries()),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const onSearchParamsChange = useCallback(
    (params: Record<string, string>) => {
      const next = new URLSearchParams();
      for (const [key, value] of Object.entries(params)) {
        if (value !== "") {
          next.set(key, value);
        }
      }
      setSearchParams(next, { replace: true });
    },
    [setSearchParams],
  );

  return (
    <ErrorBoundary>
      <Explorer
        source={source}
        home_href="/"
        initialSearchParams={initialSearchParams}
        onSearchParamsChange={onSearchParamsChange}
      />
    </ErrorBoundary>
  );
}

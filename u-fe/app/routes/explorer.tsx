// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import { useParams, useSearchParams } from "react-router";
import ErrorBoundary from "../../components/ErrorBoundary";
import { Explorer, type ExplorerGraphSource } from "../../Explorer";
import {
  readExplorerUrlParams,
  resolveOverrides,
} from "../../lib/explorerUrlParams";

export default function ExplorerRoute() {
  const { handleR, handleL } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();

  // Resolved here rather than inside Explorer so `source` stays the single
  // authority on what to load — consumers that own their own URL (the Nest app)
  // do the same resolution and pass the result in, and would otherwise have
  // these applied twice.
  const source: ExplorerGraphSource = useMemo(() => {
    const { left, right } = resolveOverrides(
      readExplorerUrlParams(searchParams),
    );
    return {
      type: "handle",
      // Both routes bind `handleR`, so it is always present.
      right: { handle: handleR ?? "", ...right },
      left: handleL != null ? { handle: handleL, ...left } : undefined,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handleR, handleL]);

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

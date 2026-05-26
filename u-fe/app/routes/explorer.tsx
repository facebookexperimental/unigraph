// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useMemo } from "react";
import { useParams, useSearchParams } from "react-router";
import ErrorBoundary from "../../components/ErrorBoundary";
import { Explorer, type ExplorerGraphSource } from "../../Explorer";

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
        home_href={home_href}
        initialSearchParams={initialSearchParams}
        onSearchParamsChange={onSearchParamsChange}
      />
    </ErrorBoundary>
  );
}

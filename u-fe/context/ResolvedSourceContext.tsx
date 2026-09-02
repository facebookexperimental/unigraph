// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext } from "react";
import type { ResolvedGraphSource } from "../Explorer";

/**
 * What the graph handles actually resolved to once the server answered.
 *
 * Handles can be anonymous (`gqc_1a2b…`) or floating (`my-timeline` = latest),
 * so the timeline and graph IDs are only known after the fetch. Panels and
 * plugins read them from here rather than re-deriving them from the props.
 */
const ResolvedSourceContext = createContext<ResolvedGraphSource | null>(null);

export function ResolvedSourceContextProvider({
  source,
  children,
}: {
  source: ResolvedGraphSource;
  children: React.ReactNode;
}) {
  return (
    <ResolvedSourceContext.Provider value={source}>
      {children}
    </ResolvedSourceContext.Provider>
  );
}

export function useResolvedSource(): ResolvedGraphSource {
  const source = useContext(ResolvedSourceContext);
  if (source == null) {
    throw new Error(
      "useResolvedSource must be used within an Explorer (ResolvedSourceContextProvider)",
    );
  }
  return source;
}

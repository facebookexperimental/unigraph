// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext, useMemo } from "react";
import type { TraversalConfig } from "../../u-be/unigraph_core/bindings/TraversalConfig";

export type TraversalConfigContextType = {
  tvc: TraversalConfig;
  setTvc: (tvc: TraversalConfig) => void;
};

const TraversalConfigContext = createContext<TraversalConfigContextType>({
  tvc: {
    force_nodes: {},
    force_edges: {},
    tag_sets: [],
    force_dynamic: [],
    tiered_traversal: null,
  },
  setTvc: () => {},
});

export function TraversalConfigContextProvider({
  children,
  tvc,
  setTvc,
}: {
  children: React.ReactNode;
  tvc: TraversalConfig;
  setTvc: (tvc: TraversalConfig) => void;
}) {
  const value = useMemo(() => ({ tvc, setTvc }), [tvc, setTvc]);

  return (
    <TraversalConfigContext.Provider value={value}>
      {children}
    </TraversalConfigContext.Provider>
  );
}

export function useTVC(): TraversalConfigContextType {
  const context = useContext(TraversalConfigContext);

  if (context === undefined) {
    throw new Error("useTVC must be used within a TraversalConfigProvider");
  }
  return context;
}

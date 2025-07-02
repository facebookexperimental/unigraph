// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "@/types";
import { createContext, useCallback, useContext, useMemo } from "react";
import type { TraversalConfig } from "../../u-be/unigraph_core/bindings/TraversalConfig";
import { useNativeGraph } from "./NativeGraphContext";

export type TraversalConfigContextType = {
  tvc: TraversalConfig;
  setTvc: (tvc: TraversalConfig) => void;
};

const TraversalConfigContext = createContext<TraversalConfigContextType | null>(
  null,
);

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

  if (context == null) {
    throw new Error("useTVC must be used within a TraversalConfigProvider");
  }
  return context;
}

export function useForceEdge(
  from: NodeIDX,
  to: NodeIDX,
): [boolean | null, (include: boolean) => void] {
  const { tvc, setTvc } = useTVC();
  const nativeGraph = useNativeGraph();

  const fromName = nativeGraph.getNodeName(from);
  const toName = nativeGraph.getNodeName(to);

  // true/false if forced. null if there is no force edge/not set
  const isForcedTo = tvc.force_edges[fromName]?.[toName]?.include ?? null;

  const forceEdge = useCallback(
    (include: boolean) => {
      setTvc({
        ...tvc,
        force_edges: {
          ...tvc.force_edges,
          [fromName]: {
            ...tvc.force_edges[fromName],
            [toName]: { include, message: null },
          },
        },
      });
    },
    [tvc, setTvc, fromName, toName],
  );

  return [isForcedTo, forceEdge];
}

export function useForceExcludeNode() {
  const { tvc, setTvc } = useTVC();
  const nativeGraph = useNativeGraph();

  return useCallback(
    (node: NodeIDX, exclude: boolean) => {
      const nodeName = nativeGraph.getNodeName(node);

      if (exclude) {
        setTvc({
          ...tvc,
          force_nodes: {
            ...tvc.force_nodes,
            [nodeName]: { include: false, message: null },
          },
        });
      } else {
        const { [nodeName]: _, ...rest } = tvc.force_nodes;
        setTvc({
          ...tvc,
          force_nodes: rest,
        });
      }
    },
    [tvc, setTvc, nativeGraph],
  );
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useCallback, useContext, useMemo } from "react";
import type { Arrow } from "@/__generated__/ts/Arrow";
import type { TraversalConfig } from "@/__generated__/ts/TraversalConfig";
import {
  ARROW_POINTS_FROM_NON_EXISTENT,
  useCanEdgeBeForcedL,
  useCanNodeBeForceExcludedL,
} from "../ArrowUtils";
import { useNativeGraphL } from "./NativeGraphContext";

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

export function useFlipForceEdgeL(arrow: Arrow | null): {
  enabled: boolean;
  forceEdge: () => void;
  action: "Include" | "Exclude";
} {
  const { tvc, setTvc } = useTVC();
  const nativeGraph = useNativeGraphL();

  const pointsTo = arrow?.points_to ?? null;
  const pointsFrom = arrow?.points_from ?? null;

  const fromName =
    pointsFrom != null && pointsFrom !== ARROW_POINTS_FROM_NON_EXISTENT
      ? nativeGraph.getNodeName(pointsFrom)
      : null;
  const toName = pointsTo != null ? nativeGraph.getNodeName(pointsTo) : null;

  const enabled = useCanEdgeBeForcedL(arrow);

  // true/false if forced. null if there is no force edge/not set
  const isForcedTo =
    fromName != null && toName != null
      ? (tvc.force_edges?.[fromName]?.[toName]?.include ?? null)
      : null;

  const action: "Include" | "Exclude" = (() => {
    if (isForcedTo === null) {
      return arrow?.excluded ? "Include" : "Exclude";
    }
    return isForcedTo ? "Exclude" : "Include";
  })();

  const forceEdge = useCallback(() => {
    if (enabled === false || fromName == null || toName == null) {
      return;
    }

    setTvc({
      ...tvc,
      force_edges: {
        ...tvc.force_edges,
        [fromName]: {
          ...tvc.force_edges?.[fromName],
          [toName]: { include: action === "Include", message_id: undefined },
        },
      },
    });
  }, [tvc, setTvc, fromName, toName, action, enabled]);

  return useMemo(() => {
    return { action, enabled, forceEdge };
  }, [action, enabled, forceEdge]);
}

export function useFlipForceExcludeNodeL(arrow: Arrow | null): {
  enabled: boolean;
  action: "Include" | "Exclude";
  forceExcludeNode: () => void;
} {
  const { tvc, setTvc } = useTVC();
  const nativeGraph = useNativeGraphL();
  const enabled = useCanNodeBeForceExcludedL(arrow);

  const pointsTo = arrow?.points_to ?? null;
  const nodeName = pointsTo != null ? nativeGraph.getNodeName(pointsTo) : null;

  const action =
    nodeName != null
      ? tvc.force_nodes?.[nodeName]?.include === false
        ? "Include"
        : "Exclude"
      : "Include";

  const forceExcludeNode = useCallback(() => {
    if (enabled === false || nodeName == null) {
      return;
    }

    if (action === "Exclude") {
      setTvc({
        ...tvc,
        force_nodes: {
          ...tvc.force_nodes,
          [nodeName]: { include: false, message_id: undefined },
        },
      });
    } else {
      const { [nodeName]: _, ...rest } = tvc.force_nodes ?? {};
      setTvc({
        ...tvc,
        force_nodes: rest,
      });
    }
  }, [tvc, setTvc, enabled, action, nodeName]);

  return useMemo(() => {
    return {
      enabled,
      action,
      forceExcludeNode,
    };
  }, [enabled, action, forceExcludeNode]);
}

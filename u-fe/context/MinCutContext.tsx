// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react";
import type { NodeIDX } from "@/types";

/// A node the user has picked to cut off in the Min Cut panel.
export interface MinCutSink {
  idx: NodeIDX;
  name: string;
}

/// An edge the user has marked as protected — the algorithm must never cut it
/// and will look for an alternative cut that routes around it.
export interface MinCutProtectedEdge {
  from: NodeIDX;
  to: NodeIDX;
}

/// Holds the Min Cut panel's selection so it survives the panel being closed
/// and reopened. Lives above `Page` (see `Explorer.tsx`) so the state persists
/// while the panel component itself mounts/unmounts. Also lets other surfaces
/// (e.g. the tree table context menu) push a node into the cut.
export type MinCutContextType = {
  sinks: MinCutSink[];
  /// Add a node to cut off. No-op if it's already selected.
  addSink: (sink: MinCutSink) => void;
  removeSink: (idx: NodeIDX) => void;
  protectedEdges: MinCutProtectedEdge[];
  /// Mark an edge as protected. No-op if it's already protected.
  protectEdge: (edge: MinCutProtectedEdge) => void;
  unprotectEdge: (edge: MinCutProtectedEdge) => void;
  /// Reset both the selected nodes and the protected edges.
  clear: () => void;
};

const MinCutContext = createContext<MinCutContextType | null>(null);

const edgeKey = (e: MinCutProtectedEdge) => `${e.from}-${e.to}`;

export function MinCutContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [sinks, setSinks] = useState<MinCutSink[]>([]);
  const [protectedEdges, setProtectedEdges] = useState<MinCutProtectedEdge[]>(
    [],
  );

  const addSink = useCallback((sink: MinCutSink) => {
    setSinks((prev) =>
      prev.some((s) => s.idx === sink.idx) ? prev : [...prev, sink],
    );
  }, []);

  const removeSink = useCallback((idx: NodeIDX) => {
    setSinks((prev) => prev.filter((s) => s.idx !== idx));
  }, []);

  const protectEdge = useCallback((edge: MinCutProtectedEdge) => {
    setProtectedEdges((prev) =>
      prev.some((e) => edgeKey(e) === edgeKey(edge)) ? prev : [...prev, edge],
    );
  }, []);

  const unprotectEdge = useCallback((edge: MinCutProtectedEdge) => {
    setProtectedEdges((prev) =>
      prev.filter((e) => edgeKey(e) !== edgeKey(edge)),
    );
  }, []);

  const clear = useCallback(() => {
    setSinks([]);
    setProtectedEdges([]);
  }, []);

  const value: MinCutContextType = useMemo(
    () => ({
      sinks,
      addSink,
      removeSink,
      protectedEdges,
      protectEdge,
      unprotectEdge,
      clear,
    }),
    [
      sinks,
      addSink,
      removeSink,
      protectedEdges,
      protectEdge,
      unprotectEdge,
      clear,
    ],
  );

  return (
    <MinCutContext.Provider value={value}>{children}</MinCutContext.Provider>
  );
}

export function useMinCut(): MinCutContextType {
  const context = useContext(MinCutContext);
  if (context == null) {
    throw new Error("useMinCut must be used within a MinCutContextProvider");
  }
  return context;
}

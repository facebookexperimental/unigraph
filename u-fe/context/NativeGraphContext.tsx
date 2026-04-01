// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext, useMemo } from "react";
import type NativeGraph from "../native/NativeGraph";
import TwinGraph from "../native/TwinGraph";

export type TNativeGraphContext = TwinGraph;
const NativeGraphContext = createContext<TNativeGraphContext | null>(null);

export function NativeGraphContextProvider({
  nativeGraphL,
  nativeGraphR,
  children,
}: {
  nativeGraphL: NativeGraph;
  nativeGraphR: NativeGraph | null;
  children: React.ReactNode;
}) {
  const value = useMemo(() => {
    return new TwinGraph(nativeGraphL, nativeGraphR);
  }, [nativeGraphL, nativeGraphR]);

  return (
    <NativeGraphContext.Provider value={value}>
      {children}
    </NativeGraphContext.Provider>
  );
}

export function useIsDeltaGraph(): boolean {
  const context = getCTX();
  return context.r != null;
}

export function useNativeGraphL(): NativeGraph {
  const context = getCTX();
  return context.l;
}

export function useNativeGraphR(): NativeGraph | null {
  const context = getCTX();
  return context.r;
}

export function useNativeGraphs(): [NativeGraph, NativeGraph | null] {
  const context = getCTX();
  return [context.l, context.r];
}

export function useTwinGraph(): TwinGraph {
  return getCTX();
}

function getCTX() {
  const context = useContext(NativeGraphContext);

  if (context == null) {
    throw new Error("useNativeGraph must be used within a NativeGraphProvider");
  }
  return context;
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext } from "react";
import type NativeGraph from "../NativeGraph";

const NativeGraphContext = createContext<NativeGraph | null>(null);

export function NativeGraphContextProvider({
  nativeGraph,
  children,
}: {
  nativeGraph: NativeGraph;
  children: React.ReactNode;
}) {
  return (
    <NativeGraphContext.Provider value={nativeGraph}>
      {children}
    </NativeGraphContext.Provider>
  );
}

export function useNativeGraph(): NativeGraph {
  const context = useContext(NativeGraphContext);

  if (context == null) {
    throw new Error("useNativeGraph must be used within a NativeGraphProvider");
  }
  return context;
}

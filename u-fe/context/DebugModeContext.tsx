// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext, useMemo, useState } from "react";

const DebugModeContext = createContext<{
  debugMode: boolean;
  setDebugMode: (debugMode: boolean) => void;
}>({
  debugMode: false,
  setDebugMode: () => {},
});

export function DebugModeContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [debugMode, setDebugMode] = useState(false);
  const value = useMemo(
    () => ({
      debugMode,
      setDebugMode,
    }),
    [debugMode],
  );

  return (
    <DebugModeContext.Provider value={value}>
      {children}
    </DebugModeContext.Provider>
  );
}

export function useDebugMode() {
  const ctx = useContext(DebugModeContext);
  if (!ctx) {
    throw new Error(
      "useDebugMode must be used within a DebugModeContextProvider",
    );
  }
  return [ctx.debugMode, ctx.setDebugMode] as const;
}

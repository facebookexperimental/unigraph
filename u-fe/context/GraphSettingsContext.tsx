// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext, useMemo } from "react";

import type { ArrayGraphSettings } from "../../u-be/unigraph_core/bindings/ArrayGraphSettings";

export type GraphSettingsContextType = [
  ArrayGraphSettings,
  (settings: ArrayGraphSettings) => void,
];

const GraphSettingsContext = createContext<GraphSettingsContextType | null>(
  null,
);

export function GraphSettingsContextProvider({
  children,
  settings,
  setSettings,
}: {
  children: React.ReactNode;
  settings: ArrayGraphSettings;
  setSettings: (settings: ArrayGraphSettings) => void;
}) {
  const value: GraphSettingsContextType = useMemo(
    () => [settings, setSettings],
    [settings, setSettings],
  );
  return (
    <GraphSettingsContext.Provider value={value}>
      {children}
    </GraphSettingsContext.Provider>
  );
}

export function useGraphSettings(): GraphSettingsContextType {
  const context = useContext(GraphSettingsContext);

  if (context == null) {
    throw new Error(
      "useGraphSettings must be used within a GraphSettingsProvider",
    );
  }
  return context;
}

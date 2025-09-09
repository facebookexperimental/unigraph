// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext, useMemo } from "react";
import type { GraphSettings } from "@/__generated__/ts/GraphSettings";

export type GraphSettingsContextType = [
  GraphSettings,
  (settings: GraphSettings) => void,
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
  settings: GraphSettings;
  setSettings: (settings: GraphSettings) => void;
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

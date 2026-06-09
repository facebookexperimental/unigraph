// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext } from "react";
import type { ExplorerPlugins } from "../Explorer";

const EMPTY_PLUGINS: ExplorerPlugins = {};

const PluginsContext = createContext<ExplorerPlugins>(EMPTY_PLUGINS);

export function PluginsContextProvider({
  plugins,
  children,
}: {
  plugins?: ExplorerPlugins;
  children: React.ReactNode;
}) {
  return (
    <PluginsContext.Provider value={plugins ?? EMPTY_PLUGINS}>
      {children}
    </PluginsContext.Provider>
  );
}

export function usePlugins(): ExplorerPlugins {
  return useContext(PluginsContext);
}

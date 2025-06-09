// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext } from "react";

export type PortalContextType = HTMLDivElement;

const PortalContext = createContext<PortalContextType | null>(null);

export function PortalContextProvider({
  containerRef,
  children,
}: {
  containerRef: HTMLDivElement | null;
  children: React.ReactNode;
}) {
  return (
    <PortalContext.Provider value={containerRef}>
      {children}
    </PortalContext.Provider>
  );
}

export function usePortalContainer(): PortalContextType | null {
  const context = useContext(PortalContext);

  if (context === undefined) {
    throw new Error(
      "usePortalContainer must be used within a PortalContextProvider and the ref",
    );
  }
  return context;
}

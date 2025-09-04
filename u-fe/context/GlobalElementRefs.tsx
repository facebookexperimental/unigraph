// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  type RefObject,
  createContext,
  useContext,
  useMemo,
  useRef,
} from "react";

type GlobalElementRefsContextType = {
  /// Ref for the search bar element. Stored globally so we
  /// can focus/blur it from anywhere on keyboard shortcuts
  nodeSearchRef: RefObject<HTMLInputElement | null>;
  /// Ref for the main explorer container div.
  /// Mostly used for rendering modals/tooltips inside it
  /// instead of using document.createElement
  portalContainerRef: RefObject<HTMLDivElement | null>;

  /// ref for the tree table (virtualized)
  treeTableRef: RefObject<HTMLDivElement | null>;
};

const GlobalElementRefsContext = createContext<
  GlobalElementRefsContextType | undefined
>(undefined);

export function GlobalElementRefsContextProvider({
  children,
}: { children: React.ReactNode }) {
  const nodeSearchRef = useRef<HTMLInputElement | null>(null);
  const portalContainerRef = useRef<HTMLDivElement | null>(null);
  const treeTableRef = useRef<HTMLDivElement | null>(null);
  const value = useMemo(
    () => ({ nodeSearchRef, portalContainerRef, treeTableRef }),
    [],
  );
  return (
    <GlobalElementRefsContext.Provider value={value}>
      {children}
    </GlobalElementRefsContext.Provider>
  );
}

function useGlobalElementRefsContext(): GlobalElementRefsContextType {
  const context = useContext(GlobalElementRefsContext);
  if (!context) {
    throw new Error(
      "GlobalElementRefsContext must be used within a GlobalElementRefsContextProvider",
    );
  }
  return context;
}

export function useNodeSearchRef(): RefObject<HTMLInputElement | null> {
  const context = useGlobalElementRefsContext();
  return context.nodeSearchRef;
}

export function usePortalContainer(): RefObject<HTMLDivElement | null> {
  const context = useGlobalElementRefsContext();
  return context.portalContainerRef;
}

export function useTreeTableRef(): RefObject<HTMLDivElement | null> {
  const context = useGlobalElementRefsContext();
  return context.treeTableRef;
}

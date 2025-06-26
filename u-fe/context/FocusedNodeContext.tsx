// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "@/types";
import { createContext, useContext, useMemo, useState } from "react";

/// Focused node is the node that is currently focused in the tree table.
export type FocusedNodeContextType = [
  NodeIDX | null,
  (setFocusedNodeIDX: NodeIDX | null) => void,
];

const FocusedNodeContext = createContext<FocusedNodeContextType | null>(null);

export function FocusedNodeContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [focusedNode, setFocusedNode] = useState<NodeIDX | null>(null);

  const value: FocusedNodeContextType = useMemo(
    () => [focusedNode, setFocusedNode],
    [focusedNode],
  );

  return (
    <FocusedNodeContext.Provider value={value}>
      {children}
    </FocusedNodeContext.Provider>
  );
}

export function useFocusedNode(): FocusedNodeContextType {
  const context = useContext(FocusedNodeContext);

  if (context == null) {
    throw new Error(
      "useFocusedNode must be used within a FocusedNodeContextProvider",
    );
  }
  return context;
}

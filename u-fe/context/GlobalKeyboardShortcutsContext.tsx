// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useCallback, useEffect, useMemo } from "react";
import {
  useToggleDominatorTreeView,
  useToggleFlatListView,
  useToggleReverseView,
} from "../GraphStructureHooks";
import { useSelectedPath } from "./SelectedPathContext";
import {
  useFlipForceEdgeL,
  useFlipForceExcludeNodeL,
} from "./TraversalConfigContext";

export const KEYBOARD_SHORTCUTS = {
  FORCE_EDGE: "e",
  FORCE_EXCLUDE_NODE: "n",
  FLAT_LIST: "f",
  REVERSE_GRAPH: "r",
  DOMINATOR_TREE: "d",
};

type GlobalKeyboardShortcutsContextType = {};

const GlobalKeyboardShortcutsContext = createContext<
  GlobalKeyboardShortcutsContextType | undefined
>(undefined);

export function GlobalKeyboardShortcutsContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const value = useMemo(() => ({}), []);
  const handler = useExplorerKeyboardShortcutsHandler();

  useEffect(() => {
    // add handler to global document when the component mounts
    const handleKeyDown = (e: KeyboardEvent) => {
      handler(e);
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [handler]);

  return (
    <GlobalKeyboardShortcutsContext.Provider value={value}>
      {children}
    </GlobalKeyboardShortcutsContext.Provider>
  );
}

export function useExplorerKeyboardShortcutsHandler(): (
  e: KeyboardEvent,
) => void {
  const { selectedRow } = useSelectedPath();
  const arrow = selectedRow?.arrow_pair || null;

  const arrowL = arrow?.l ?? null;

  const flipForceEdge = useFlipForceEdgeL(arrowL);
  const flipForceExcludeNode = useFlipForceExcludeNodeL(arrowL);
  const [_f, toggleFlatList] = useToggleFlatListView();
  const [_r, toggleReverseView] = useToggleReverseView();
  const [_d, toggleDominatorTreeView] = useToggleDominatorTreeView();

  const keyboardEventHandler = useCallback(
    (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();

      const modifiers = e.ctrlKey || e.metaKey;
      if (modifiers) {
        // Ignore shortcuts with modifiers, otherwise it will conflict with
        // other browser shortcuts (like Cmd+R to refresh will trigger REVERSE_GRAPH)
        return;
      }

      switch (key) {
        case KEYBOARD_SHORTCUTS.FORCE_EXCLUDE_NODE: {
          if (flipForceExcludeNode.enabled) {
            flipForceExcludeNode.forceExcludeNode();
          }
          break;
        }
        case KEYBOARD_SHORTCUTS.FORCE_EDGE: {
          if (flipForceEdge.enabled) {
            flipForceEdge.forceEdge();
          }
          break;
        }
        case KEYBOARD_SHORTCUTS.FLAT_LIST: {
          toggleFlatList();
          break;
        }
        case KEYBOARD_SHORTCUTS.REVERSE_GRAPH: {
          toggleReverseView();
          break;
        }
        case KEYBOARD_SHORTCUTS.DOMINATOR_TREE: {
          toggleDominatorTreeView();
          break;
        }
      }
    },
    [
      flipForceEdge,
      flipForceExcludeNode,
      toggleFlatList,
      toggleReverseView,
      toggleDominatorTreeView,
    ],
  );

  return keyboardEventHandler;
}

export function KeyboardShortcutLabel({ label }: { label: string }) {
  return (
    <span className="text-xs font-bold text-background rounded px-1 mx-1 bg-foreground">
      <kbd className="kbd">{label}</kbd>
    </span>
  );
}

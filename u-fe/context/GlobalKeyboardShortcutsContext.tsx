// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect } from "react";
import {
  useToggleDominatorTreeView,
  useToggleFlatListView,
  useToggleReverseView,
} from "../GraphStructureHooks";
import { useNodeSearchRef } from "./GlobalElementRefs";
import { useSelectedPath } from "./SelectedPathContext";
import {
  useFlipForceEdgeL,
  useFlipForceExcludeNodeL,
} from "./TraversalConfigContext";

export type TShortcutDefinition = {
  key: string;
  cmd: boolean;
};

export const KEYBOARD_SHORTCUTS = {
  FORCE_EDGE: { key: "e", cmd: false },
  FORCE_EXCLUDE_NODE: { key: "n", cmd: false },
  FLAT_LIST: { key: "f", cmd: false },
  REVERSE_GRAPH: { key: "r", cmd: false },
  DOMINATOR_TREE: { key: "d", cmd: false },
  NODE_SEARCH: { key: "k", cmd: true },
} as const satisfies Record<string, TShortcutDefinition>;

type ShortcutNames = keyof typeof KEYBOARD_SHORTCUTS;

export function useGlobalKeyboardShortcuts() {
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
}

export function useExplorerKeyboardShortcutsHandler(): (
  e: KeyboardEvent,
) => void {
  const { selectedRow } = useSelectedPath();
  const nodeSearchRef = useNodeSearchRef();
  const arrow = selectedRow?.twinArrow || null;

  const arrowL = arrow?.l ?? null;

  const flipForceEdge = useFlipForceEdgeL(arrowL);
  const flipForceExcludeNode = useFlipForceExcludeNodeL(arrowL);
  const [_f, toggleFlatList] = useToggleFlatListView();
  const [_r, toggleReverseView] = useToggleReverseView();
  const [_d, toggleDominatorTreeView] = useToggleDominatorTreeView();

  const keyboardEventHandler = useCallback(
    (e: KeyboardEvent) => {
      // Skip shortcuts when the user is typing in a text field — otherwise
      // single-key shortcuts like "f" (flat list) or "e" (force edge) would
      // fire while editing node names, tag names, etc. in the sidebar panels.
      const target = e.target;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }

      const key = e.key.toLowerCase();

      const cmdPressed = e.ctrlKey || e.metaKey;

      for (const [name, shortcut] of Object.entries(KEYBOARD_SHORTCUTS)) {
        if (shortcut.key === key && shortcut.cmd === cmdPressed) {
          const shortcutName = name as ShortcutNames;
          switch (shortcutName) {
            case "FORCE_EDGE":
              if (flipForceEdge.enabled) {
                flipForceEdge.forceEdge();
              }
              break;
            case "FORCE_EXCLUDE_NODE":
              if (flipForceExcludeNode.enabled) {
                flipForceExcludeNode.forceExcludeNode();
              }
              break;
            case "FLAT_LIST":
              toggleFlatList();
              break;
            case "REVERSE_GRAPH":
              toggleReverseView();
              break;
            case "DOMINATOR_TREE":
              toggleDominatorTreeView();
              break;
            case "NODE_SEARCH": {
              console.log("node search");
              nodeSearchRef.current?.focus();
              break;
            }
            default: {
              const exhaustiveCheck: never = shortcutName;
              console.error(`Unexpected shortcut: ${exhaustiveCheck}`);
            }
          }
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
      nodeSearchRef,
    ],
  );

  return keyboardEventHandler;
}

export function KeyboardShortcutLabel({
  shortcut,
}: {
  shortcut: TShortcutDefinition;
}) {
  const cmdSpan = shortcut.cmd ? (
    <span className="text-xs font-bold">⌘</span>
  ) : null;
  return (
    <span className="text-xs font-bold text-background rounded px-1 mx-1 bg-foreground">
      {cmdSpan}
      <kbd className="kbd">{shortcut.key.toUpperCase()}</kbd>
    </span>
  );
}

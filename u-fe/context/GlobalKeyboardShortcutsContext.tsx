// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  type RefObject,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
} from "react";
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

type GlobalKeyboardShortcutsContextType = {
  /// Ref for the search bar element. Stored globally so we
  /// can focus/blur it from anywhere on keyboard shortcuts
  nodeSearchRef: RefObject<HTMLInputElement | null>;
};

const GlobalKeyboardShortcutsContext = createContext<
  GlobalKeyboardShortcutsContextType | undefined
>(undefined);

export function GlobalKeyboardShortcutsContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const nodeSearchRef = useRef<HTMLInputElement | null>(null);
  const value = useMemo(() => ({ nodeSearchRef }), []);
  const handler = useExplorerKeyboardShortcutsHandler(value);

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

function useGlobalKeyboardShortcutsContext(): GlobalKeyboardShortcutsContextType {
  const context = useContext(GlobalKeyboardShortcutsContext);
  if (!context) {
    throw new Error(
      "useGlobalKeyboardShortcutsContext must be used within a GlobalKeyboardShortcutsContextProvider",
    );
  }
  return context;
}

export function useNodeSearchRef(): RefObject<HTMLInputElement | null> {
  const context = useGlobalKeyboardShortcutsContext();
  return context.nodeSearchRef;
}

export function useExplorerKeyboardShortcutsHandler(
  ctx: GlobalKeyboardShortcutsContextType,
): (e: KeyboardEvent) => void {
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
              ctx.nodeSearchRef.current?.focus();
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
      ctx,
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

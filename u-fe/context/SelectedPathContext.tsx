// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { Row } from "@/tree_table/TreeTableRows";
import type { NodeIDX } from "@/types";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type TwinGraph from "../native/TwinGraph";
import { useTwinGraph } from "./NativeGraphContext";

export type SelectedPathContextType = {
  selectedPath: NodeIDX[] | null;
  setSelectedPath: (
    setSelectedPathIDX: NodeIDX[] | null,
    navigate?: boolean,
  ) => void;

  // selected row is not synced with URL, it's a derived state
  // that we would normally need to use in different places. e.g.
  // triggering certain actions on keyboard shortcuts.
  selectedRow: Readonly<Row | null>;
  setSelectedRow: (row: Row | null) => void;
  pathSelector: TreeTablePathSelector;
};

const SelectedPathContext = createContext<SelectedPathContextType | null>(null);

export function SelectedPathContextProvider({
  children,
  syncToURL,
}: {
  children: React.ReactNode;
  syncToURL: boolean;
}) {
  const twinGraph = useTwinGraph();
  const pathSelector = useRef(new TreeTablePathSelector());

  const [selectedPath, setSelectedPath] = useState<NodeIDX[] | null>(() => {
    return syncToURL ? parseSelectedPathFromURLHash(twinGraph) : null;
  });

  const [selectedRow, setSelectedRow] = useState<Row | null>(null);

  useEffect(() => {
    if (syncToURL) {
      syncSelectedPathToURLHash(twinGraph, selectedPath);
    }
  }, [twinGraph, selectedPath, syncToURL]);

  const value: SelectedPathContextType = useMemo(() => {
    return {
      selectedPath,
      setSelectedPath: (
        newSelectedPathIDX: NodeIDX[] | null,
        navigate?: boolean,
      ) => {
        setSelectedPath(newSelectedPathIDX);
        if (navigate) {
          pathSelector.current.navigate(newSelectedPathIDX);
        }
      },
      selectedRow,
      setSelectedRow: (row: Row | null) => {
        setSelectedRow(row);
      },
      pathSelector: pathSelector.current,
    };
  }, [selectedPath, selectedRow]);

  return (
    <SelectedPathContext.Provider value={value}>
      {children}
    </SelectedPathContext.Provider>
  );
}

export function useSelectedPath(): SelectedPathContextType {
  const context = useContext(SelectedPathContext);

  if (context == null) {
    throw new Error(
      "useSelectedPath must be used within a SelectedPathContextProvider",
    );
  }
  return context;
}

export function useSelectedNodeIDX(): NodeIDX | null {
  const { selectedPath } = useSelectedPath();
  if (selectedPath == null || selectedPath.length === 0) {
    return null;
  }
  return selectedPath[selectedPath.length - 1] ?? null;
}

function syncSelectedPathToURLHash(
  twinGraph: TwinGraph,
  selectedPath: NodeIDX[] | null,
) {
  if (selectedPath == null || selectedPath.length === 0) {
    // If the selected path is empty, remove the hash from the URL
    window.history.replaceState(null, "", window.location.href.split("#")[0]);
    return;
  }

  const nodeNamePath = selectedPath.map((idx) => twinGraph.getNodeName(idx));
  const serialized = JSON.stringify(nodeNamePath);
  const encoded = encodeURIComponent(serialized);
  // update the hash of the URL only with the new encoded value
  const newHash = `#${encoded}`;
  window.history.replaceState(null, "", newHash);
}

function parseSelectedPathFromURLHash(twinGraph: TwinGraph): NodeIDX[] | null {
  const hash = window.location.hash;
  if (hash.length === 0) {
    return null;
  }
  const decoded = decodeURIComponent(hash.slice(1));
  const parsed = JSON.parse(decoded);
  if (!Array.isArray(parsed)) {
    return null;
  }
  const nodeNamePath: string[] = parsed;
  const nodeIDXPath: NodeIDX[] = [];
  for (const nodeName of nodeNamePath) {
    const nodeIDX = twinGraph.getNodeIDXByNameLog(nodeName);
    if (nodeIDX == null) {
      // We'll try to parse as far as possible. If something
      // is missing in the middle we'll return whatever we have.
      return nodeIDXPath;
    }
    nodeIDXPath.push(nodeIDX);
  }
  return nodeIDXPath;
}

// Some super sketchy stuff to make it possible to
// select a new path from the outside of the component.
// There's probably a much better way to do this, but i have
// no idea how so here we are.
// This class will be created in the outside world and passed
// down to the TreeTable component.
// If someone calls `navigate` on this class, it will
// make TreeTable do all the expanding/scrolling to the new
//  selected path/node.
export class TreeTablePathSelector {
  navigate: (path: NodeIDX[] | null) => void;

  constructor() {
    this.navigate = () => {};
  }
}

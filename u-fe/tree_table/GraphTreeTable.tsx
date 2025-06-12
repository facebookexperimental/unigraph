// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "../types";
import { TreeTable, TreeTablePathSelector } from "./TreeTable";

import { useCallback, useEffect, useRef } from "react";
import type { GraphTableSort } from "u-be/unigraph_core/bindings/GraphTableSort";
import type NativeGraph from "../NativeGraph";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useNativeGraph } from "../context/NativeGraphContext";
import useGraphTreeTableColumns from "./useGraphTreeTableColumns";

export default function GraphTreeTable(props: {
  roots: Readonly<NodeIDX[]>;
  focusOnMount?: boolean;
}) {
  const nativeGraph = useNativeGraph();
  const [settings, setSettings] = useGraphSettings();

  const onSortChange = useCallback(
    (sort: GraphTableSort | null) => {
      setSettings({
        ...settings,
        ui_settings: {
          ...settings.ui_settings,
          graph_table_sort: sort == null ? undefined : sort,
        },
      });
    },
    [settings, setSettings],
  );

  const onSelectedNodeIDXPathChange = useCallback(
    (path: NodeIDX[]) => {
      syncSelectedPathToURLHash(nativeGraph, path);
    },
    [nativeGraph],
  );

  const pathSelector = useRef(
    new TreeTablePathSelector(parseSelectedPathFromURLHash(nativeGraph)),
  );
  useEffect(() => {
    const path = parseSelectedPathFromURLHash(nativeGraph);
    if (path) {
      pathSelector.current.setNewSelectedPath(path);
    }
  }, [nativeGraph]);

  const columnDefinitions = useGraphTreeTableColumns();

  const getArrows = useCallback(
    (nodeIDX: NodeIDX) => {
      return nativeGraph.getArrowsForward(nodeIDX);
    },
    [nativeGraph],
  );

  return (
    <TreeTable
      roots={props.roots}
      columnDefinitions={columnDefinitions}
      getArrows={getArrows}
      focusOnMount={props.focusOnMount}
      onSortChange={onSortChange}
      sortColumnID={settings?.ui_settings?.graph_table_sort?.column_id ?? null}
      sortOrder={settings?.ui_settings?.graph_table_sort?.order ?? null}
      pathSelector={pathSelector.current}
      onSelectedNodeIDXPathChange={onSelectedNodeIDXPathChange}
    />
  );
}

function syncSelectedPathToURLHash(
  nativeGraph: NativeGraph,
  selectedPath: NodeIDX[],
) {
  const nodeNamePath = selectedPath.map((idx) => nativeGraph.getNodeName(idx));
  const serialized = JSON.stringify(nodeNamePath);
  const encoded = encodeURIComponent(serialized);
  // update the hash of the URL only with the new encoded value
  const newHash = `#${encoded}`;
  window.history.replaceState(null, "", newHash);
}

function parseSelectedPathFromURLHash(
  nativeGraph: NativeGraph,
): NodeIDX[] | null {
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
    const nodeIDX = nativeGraph.getNodeIDXByNameLog(nodeName);
    if (nodeIDX == null) {
      // We'll try to parse as far as possible. If something
      // is missing in the middle we'll return whatever we have.
      return nodeIDXPath;
    }
    nodeIDXPath.push(nodeIDX);
  }
  return nodeIDXPath;
}

// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "../types";
import {
  type ColumnDefinitions,
  type NonTreeColumnDefinition,
  type NumericValueColumnDefinition,
  type Sort,
  type TreeColumnDefinition,
  TreeTable,
  TreeTablePathSelector,
} from "./TreeTable";

import { useCallback, useEffect, useMemo, useRef } from "react";
import { usePageParams } from "../PageParams";
import type NativeGraph from "../NativeGraph";
import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import ContextMenuCell from "./ContextMenuCell";
import formatNumber from "../lib/formatNumber";

export default function GraphTreeTable(props: {
  roots: NodeIDX[];
  focusOnMount?: boolean;
  nativeGraph: NativeGraph;
}) {
  const columnDefinitions: ColumnDefinitions = useMemo(() => {
    const columnDefinitions: { [name: string]: NonTreeColumnDefinition } = {};
    for (const metricName of props.nativeGraph.metricNames) {
      const definition: NumericValueColumnDefinition = {
        t: "numeric_value_column",
        name: metricName,
        renderer: (arrow: Arrow) => {
          const value = arrow.excluded
            ? "-"
            : formatNumber(
                props.nativeGraph.getNodeMetric(arrow.points_to, metricName),
                0,
                true,
              );
          return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
        },
        getNumericValues: (idxs: NodeIDX[]) =>
          props.nativeGraph.getNodeMetricBatched(idxs, metricName),
        sortable: true,
      };
      columnDefinitions[metricName] = definition;

      const transitiveMetricColumnName = `${metricName} (transitive)`;
      const transitiveDefinition: NumericValueColumnDefinition = {
        t: "numeric_value_column",
        name: transitiveMetricColumnName,
        renderer: (arrow: Arrow) => {
          const value = arrow.excluded
            ? "-"
            : formatNumber(
                props.nativeGraph.getTransitiveMetric(
                  arrow.points_to,
                  metricName,
                ),
                0,
                true,
              );
          return <p className="px-4 text-right tabular-nums w-full">{value}</p>;
        },
        getNumericValues: (idxs: NodeIDX[]) => {
          return props.nativeGraph.getTransitiveMetricsBatched(
            idxs,
            metricName,
          );
        },
        sortable: true,
      };
      columnDefinitions[transitiveMetricColumnName] = transitiveDefinition;
    }

    const treeColumn: TreeColumnDefinition = {
      name: "TreeColumn",
      getNodeName: (idx: NodeIDX) => props.nativeGraph.getNodeName(idx),
      flexGrow: 1,
    };

    columnDefinitions.context_menu = {
      t: "non_sortable_column",
      name: "",
      renderer: (arrow: Arrow) => (
        <ContextMenuCell arrow={arrow} nativeGraph={props.nativeGraph} />
      ),
    };

    return {
      treeColumn,
      columns: columnDefinitions,
    };
  }, [props.nativeGraph]);

  const [pageParams, setPageParams] = usePageParams();

  const onSortChange = useCallback(
    (sort: Sort | null) => {
      setPageParams({
        graphTableSort: sort == null ? undefined : sort,
      });
    },
    [setPageParams],
  );

  const onSelectedNodeIDXPathChange = useCallback(
    (path: NodeIDX[]) => {
      syncSelectedPathToURLHash(props.nativeGraph, path);
    },
    [props.nativeGraph],
  );

  const pathSelector = useRef(
    new TreeTablePathSelector(parseSelectedPathFromURLHash(props.nativeGraph)),
  );
  useEffect(() => {
    const path = parseSelectedPathFromURLHash(props.nativeGraph);
    if (path) {
      pathSelector.current.setNewSelectedPath(path);
    }
  }, [props.nativeGraph]);

  const getArrows = useCallback(
    (nodeIDX: NodeIDX) => {
      return props.nativeGraph.getArrowsForward(nodeIDX);
    },
    [props.nativeGraph],
  );

  return (
    <TreeTable
      roots={props.roots}
      columnDefinitions={columnDefinitions}
      getArrows={getArrows}
      focusOnMount={props.focusOnMount}
      onSortChange={onSortChange}
      sortColumnName={pageParams.graphTableSort?.columnName ?? null}
      sortOrder={pageParams.graphTableSort?.order ?? null}
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

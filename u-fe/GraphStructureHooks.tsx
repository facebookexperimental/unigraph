// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback } from "react";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "u-be/unigraph_core/bindings/ArrayGraphUISettingsTreeTableEntryPoints";
import type { GraphStructure } from "u-be/unigraph_core/bindings/GraphStructure";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraph } from "./context/NativeGraphContext";
import { useSelectedNodeIDX } from "./context/SelectedPathContext";

export function useToggleFlatListView(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const entryPoints = useGraphEntryPoints();
  const checked = entryPoints === "AllReachable";

  const toggle = useCallback(() => {
    setGraphSettings({
      ...graphSettings,
      ui_settings: {
        ...graphSettings.ui_settings,
        entry_points: checked ? "Determine" : "AllReachable",
      },
    });
  }, [graphSettings, setGraphSettings, checked]);

  return [checked, toggle];
}

export function useToggleReverseView(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const structure = useGraphStructure();
  const entryPoints = useGraphEntryPoints();
  const selectedNodeIDX = useSelectedNodeIDX();
  const nativeGraph = useNativeGraph();
  const checked = structure === "Reverse";

  const toggle = useCallback(() => {
    const newChecked = !checked;
    const [newEntryPoints, entry_points_specified] = (() => {
      if (newChecked) {
        if (entryPoints === "AllReachable") {
          /// if we're in a flat list we don't want to change entry points
          /// Our dependency will already be there in the root
          return ["AllReachable" as const, undefined];
        } else {
          if (selectedNodeIDX == null) {
            // if we don't have a selected node, we want to show all reachable nodes
            return ["AllReachable" as const, undefined];
          }

          // if we have a selected node and we are turning the reverse graph on outside
          // of a flat list we will make that selected node the entry point for the tree
          // table.
          return [
            "Specified" as const,
            [nativeGraph.getNodeName(selectedNodeIDX)],
          ];
        }
      } else {
        if (entryPoints === "AllReachable") {
          /// if we're in a flat list we should stay in a flat list
          return ["AllReachable" as const, undefined];
        } else {
          // if we're unchecking the reverse graph, we will switch back to
          // the default forward graph.
          return ["Determine" as const, undefined];
        }
      }
    })();

    setGraphSettings({
      ...graphSettings,
      ui_settings: {
        ...graphSettings.ui_settings,
        graph_structure: newChecked ? "Reverse" : "Forward",
        entry_points: newEntryPoints,
        entry_points_specified,
      },
    });
  }, [
    graphSettings,
    setGraphSettings,
    checked,
    entryPoints,
    selectedNodeIDX,
    nativeGraph,
  ]);

  return [checked, toggle];
}

export function useToggleDominatorTreeView(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const structure = useGraphStructure();
  const checked = structure === "Dominator";

  const toggle = useCallback(() => {
    setGraphSettings({
      ...graphSettings,
      ui_settings: {
        ...graphSettings.ui_settings,
        graph_structure: checked ? "Forward" : "Dominator",
      },
    });
  }, [graphSettings, setGraphSettings, checked]);

  return [checked, toggle];
}

export function useGraphStructure(): GraphStructure {
  const [graphSettings] = useGraphSettings();
  return graphSettings.ui_settings?.graph_structure ?? "Forward";
}

export function useGraphEntryPoints(): ArrayGraphUISettingsTreeTableEntryPoints {
  const [graphSettings] = useGraphSettings();
  return graphSettings.ui_settings?.entry_points ?? "Determine";
}

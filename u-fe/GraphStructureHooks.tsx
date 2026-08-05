// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback } from "react";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { EntryPointsFilter } from "./__generated__/ts/EntryPointsFilter";
import type { GraphStructure } from "./__generated__/ts/GraphStructure";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraphR } from "./context/NativeGraphContext";
import { useSelectedNodeIDX } from "./context/SelectedPathContext";

export const EMPTY_ENTRY_POINTS_FILTER: EntryPointsFilter = {
  properties: {},
  incoming_tags: [],
  incoming_dynamic_type_keys: [],
};

/// Both modes render every matching node as its own root, so anything that
/// asks "are we in a flat list?" has to accept `Filtered` too.
export function isFlatListEntryPoints(
  entryPoints: ArrayGraphUISettingsTreeTableEntryPoints,
): boolean {
  return entryPoints === "AllReachable" || entryPoints === "Filtered";
}

export function isEntryPointsFilterEmpty(filter: EntryPointsFilter): boolean {
  return (
    Object.keys(filter.properties).length === 0 &&
    filter.incoming_tags.length === 0 &&
    filter.incoming_dynamic_type_keys.length === 0
  );
}

export function useToggleFlatListView(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const entryPoints = useGraphEntryPoints();
  const checked = isFlatListEntryPoints(entryPoints);

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

export function useEntryPointsFilter(): EntryPointsFilter {
  const [graphSettings] = useGraphSettings();
  return (
    graphSettings.ui_settings?.entry_points_filter ?? EMPTY_ENTRY_POINTS_FILTER
  );
}

/// Commit a new filter. Setting any condition switches the tree table into
/// `Filtered`; clearing the last one drops back to the default entry points.
export function useSetEntryPointsFilter(): (filter: EntryPointsFilter) => void {
  const [graphSettings, setGraphSettings] = useGraphSettings();

  return useCallback(
    (filter: EntryPointsFilter) => {
      const hasConditions = !isEntryPointsFilterEmpty(filter);
      setGraphSettings({
        ...graphSettings,
        ui_settings: {
          ...graphSettings.ui_settings,
          entry_points: hasConditions ? "Filtered" : "Determine",
          entry_points_filter: hasConditions ? filter : undefined,
        },
      });
    },
    [graphSettings, setGraphSettings],
  );
}

export function useToggleReverseView(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const structure = useGraphStructure();
  const entryPoints = useGraphEntryPoints();
  const selectedNodeIDX = useSelectedNodeIDX();
  const nativeGraphR = useNativeGraphR();
  const checked = structure === "Reverse";

  const toggle = useCallback(() => {
    const newChecked = !checked;
    const [newEntryPoints, entry_points_specified] = (() => {
      if (newChecked) {
        if (isFlatListEntryPoints(entryPoints)) {
          /// if we're in a flat list we don't want to change entry points
          /// Our dependency will already be there in the root
          return [entryPoints, undefined];
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
            [nativeGraphR.getNodeName(selectedNodeIDX)],
          ];
        }
      } else {
        if (isFlatListEntryPoints(entryPoints)) {
          /// if we're in a flat list we should stay in a flat list
          return [entryPoints, undefined];
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
    nativeGraphR,
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

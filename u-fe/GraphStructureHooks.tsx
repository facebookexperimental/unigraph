// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback } from "react";
import type { ArrayGraphUISettingsTreeTableEntryPoints } from "./__generated__/ts/ArrayGraphUISettingsTreeTableEntryPoints";
import type { NodeSelection } from "./__generated__/ts/NodeSelection";
import type { GraphStructure } from "./__generated__/ts/GraphStructure";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraphR } from "./context/NativeGraphContext";
import { useSelectedNodeIDX } from "./context/SelectedPathContext";

export const EMPTY_ENTRY_POINTS_FILTER: NodeSelection = {
  properties: {},
  incoming_tags: [],
  incoming_dynamic_type_keys: [],
  outgoing_tags: [],
  outgoing_dynamic_type_keys: [],
};

/// Whether the tree table renders every matching node as its own root, which
/// both flat-list modes do — so structural questions ("can a path be valid?",
/// "do entry points survive a reverse toggle?") have to accept `Filtered` too.
///
/// Not the same question as which footer button lights up: `AllReachable` and
/// `Filtered` are distinct modes and their toggles are mutually exclusive.
export function isFlatListEntryPoints(
  entryPoints: ArrayGraphUISettingsTreeTableEntryPoints,
): boolean {
  return entryPoints === "AllReachable" || entryPoints === "Filtered";
}

export function isEntryPointsFilterEmpty(filter: NodeSelection): boolean {
  return countEntryPointsFilterConditions(filter) === 0;
}

export function countEntryPointsFilterConditions(
  filter: NodeSelection,
): number {
  return (
    (hasNameCondition(filter) ? 1 : 0) +
    Object.keys(filter.properties).length +
    filter.incoming_tags.length +
    filter.incoming_dynamic_type_keys.length +
    filter.outgoing_tags.length +
    filter.outgoing_dynamic_type_keys.length
  );
}

/// Mirrors `NodeSelection::name_condition` on the Rust side: a blank
/// pattern is a condition the user started and abandoned, not one that matches
/// nothing, so it must not count or keep you in `Filtered`.
export function hasNameCondition(filter: NodeSelection): boolean {
  return (filter.name?.pattern ?? "").trim().length > 0;
}

/// The unfiltered flat list. Mutually exclusive with [`useToggleFilteredFlatList`]
/// — `AllReachable` and `Filtered` are different modes, so only one of the two
/// footer buttons is ever lit. Pressing this one while filtering drops the
/// filter and shows everything.
export function useToggleFlatListView(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const checked = useGraphEntryPoints() === "AllReachable";

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

export function useEntryPointsFilter(): NodeSelection {
  const [graphSettings] = useGraphSettings();
  return (
    graphSettings.ui_settings?.entry_points_filter ?? EMPTY_ENTRY_POINTS_FILTER
  );
}

/// The filtered flat list. Mutually exclusive with [`useToggleFlatListView`]:
/// turning this on takes you from "show all" to "show only matching", so the
/// plain flat-list button goes dark.
///
/// Turning it on from outside a flat list moves you into one — filtering only
/// has meaning over the flat list. Turning it off leaves you in the flat list
/// rather than the determined tree, and leaves `entry_points_filter` in place,
/// so the conditions are still there when you toggle back.
export function useToggleFilteredFlatList(): [boolean, () => void] {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const checked = useGraphEntryPoints() === "Filtered";

  const toggle = useCallback(() => {
    setGraphSettings({
      ...graphSettings,
      ui_settings: {
        ...graphSettings.ui_settings,
        entry_points: checked ? "AllReachable" : "Filtered",
      },
    });
  }, [graphSettings, setGraphSettings, checked]);

  return [checked, toggle];
}

/// Commit a new filter. Setting the first condition switches the tree table
/// into `Filtered` so the effect is immediate; clearing the last one drops back
/// to the plain flat list, which is where the filter toggle would leave you.
export function useSetEntryPointsFilter(): (filter: NodeSelection) => void {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const entryPoints = useGraphEntryPoints();

  return useCallback(
    (filter: NodeSelection) => {
      const hasConditions = !isEntryPointsFilterEmpty(filter);
      setGraphSettings({
        ...graphSettings,
        ui_settings: {
          ...graphSettings.ui_settings,
          entry_points: nextEntryPoints(entryPoints, hasConditions),
          entry_points_filter: hasConditions ? filter : undefined,
        },
      });
    },
    [graphSettings, setGraphSettings, entryPoints],
  );
}

/// Emptying the filter only moves you when the filter is what you're looking
/// at. Editing conditions from outside `Filtered` — with the popover open over
/// a tree, say — must not yank the tree table somewhere else.
function nextEntryPoints(
  entryPoints: ArrayGraphUISettingsTreeTableEntryPoints,
  hasConditions: boolean,
): ArrayGraphUISettingsTreeTableEntryPoints {
  if (hasConditions) {
    return "Filtered";
  }
  return entryPoints === "Filtered" ? "AllReachable" : entryPoints;
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

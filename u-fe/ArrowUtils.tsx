// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { Arrow } from "@/__generated__/ts/Arrow";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraph } from "./context/NativeGraphContext";
import { useTVC } from "./context/TraversalConfigContext";

// JS indexes are not u32 so -1 is a valid value.
// We're going to abuse it to represent the root arrows that technically
// don't have parents
export const ARROW_POINTS_FROM_NON_EXISTENT = -1;

export function useCanEdgeBeForced(arrow: Arrow | null): boolean {
  const nativeGraph = useNativeGraph();

  const isExclusionEnabledForGraphStructure =
    useIsExclusionEnabledForGraphStructure();

  if (!isExclusionEnabledForGraphStructure) {
    return false;
  }

  if (arrow == null) {
    return false;
  }

  // if it's an entrypoint it gets weird, so we'll disable the ability to force it
  const isEntryoint = nativeGraph
    .determineEntrypoints()
    .set.has(arrow.points_to);

  // If the points_from is non-existent that means there is no real edge
  // and we can't force it.
  const isRootArrow = arrow.points_from === ARROW_POINTS_FROM_NON_EXISTENT;

  return !isEntryoint && !isRootArrow;
}

export function useIsExclusionEnabledForGraphStructure(): boolean {
  const [graphSettings] = useGraphSettings();

  const structure = graphSettings?.ui_settings?.graph_structure ?? "Forward";

  if (structure !== "Forward") {
    // in dominator tree the rows we render do not correspond to the
    // nodes in the graph, so we can't exclude them really.
    // for reverse we can technically do that if we flip `to` and `from`
    // but we won't do it for now.
    return false;
  }
  return true;
}

export function useCanNodeBeForceExcluded(arrow: Arrow | null): boolean {
  const nativeGraph = useNativeGraph();
  const { tvc } = useTVC();

  const isExclusionEnabledForGraphStructure =
    useIsExclusionEnabledForGraphStructure();
  if (!isExclusionEnabledForGraphStructure) {
    return false;
  }

  if (arrow == null) {
    return false;
  }

  const nodeName = nativeGraph.getNodeName(arrow.points_to);

  const isEntryoint = nativeGraph
    .determineEntrypoints()
    .set.has(arrow.points_to);

  if (isEntryoint) {
    // it gets super weird if you exclude an entrypoint.
    // If it's the onlyone we'll get in the state where there
    // is no graph at all.
    return false;
  }

  const nodeForce = tvc.force_nodes?.[nodeName] ?? null;
  const reachable = nativeGraph.isNodeReachable(arrow.points_to);

  if (nodeForce == null && !reachable) {
    // if node is not forced now and not reachable then even if we exclude
    // it nothing is gonna happen.
    return false;
  }

  return true;
}

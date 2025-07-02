// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import { useNativeGraph } from "./context/NativeGraphContext";
import { useTVC } from "./context/TraversalConfigContext";

// JS indexes are not u32 so -1 is a valid value.
// We're going to abuse it to represent the root arrows that technically
// don't have parents
export const ARROW_POINTS_FROM_NON_EXISTENT = -1;

export function canArrowBeForced(arrow: Arrow): boolean {
  const nativeGraph = useNativeGraph();

  // if it's an entrypoint it gets weird, so we'll disable the ability to force it
  const isEntryoint = nativeGraph
    .determineEntrypoints()
    .set.has(arrow.points_to);

  // If the points_from is non-existent that means there is no real edge
  // and we can't force it.
  const isRootArrow = arrow.points_from === ARROW_POINTS_FROM_NON_EXISTENT;

  return !isEntryoint && !isRootArrow;
}

export function canNodeBeForceExcluded(arrow: Arrow): boolean {
  const nativeGraph = useNativeGraph();
  const { tvc } = useTVC();

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

  const nodeForce = tvc.force_nodes[nodeName] ?? null;
  const reachable = nativeGraph.isNodeReachable(arrow.points_to);

  if (nodeForce == null && !reachable) {
    // if node is not forced now and not reachable then even if we exclude
    // it nothing is gonna happen.
    return false;
  }

  return true;
}

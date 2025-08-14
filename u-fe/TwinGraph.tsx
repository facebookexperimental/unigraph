// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  get_arrow_pairs,
  node_idx_to_name,
  node_name_to_idx_log,
} from "../.build/wasm/unigraph_wasm";
import type NativeGraph from "./NativeGraph";
import type { GraphStructureU8, NodeIDXVecSet } from "./NativeGraph";
import type { Arrow } from "./__generated__/ts/Arrow";
import type { NodeIDX } from "./types";

// This is a wrapper class over One or Two Native Graphs (left + ?right)

/// Matched arrows pair represents either a single arrow if we have a single graph
/// or two optional arrows if we're comparing two graphs.
/// There should not be a situation where we have both arrows null.
///
/// if we have two arrows they must BOTH point TO and FROM the same node
export type ArrowPair =
  | {
      points_to: NodeIDX;
      points_from: NodeIDX;
      l: Arrow;
      r: Arrow;
    }
  | {
      points_to: NodeIDX;
      points_from: NodeIDX;
      l: Arrow;
      r: Arrow | null;
    }
  | {
      points_to: NodeIDX;
      points_from: NodeIDX;
      l: Arrow | null;
      r: Arrow;
    };

export default class TwinGraph {
  readonly l: NativeGraph;
  readonly r: NativeGraph | null;
  private entrypointsCache: NodeIDXVecSet | null = null;

  constructor(l: NativeGraph, r: NativeGraph | null) {
    this.l = l;
    this.r = r;
  }

  isDeltaGraph(): boolean {
    return this.r != null;
  }

  determineEntrypoints() {
    if (this.entrypointsCache == null) {
      if (this.r == null) {
        this.entrypointsCache = this.l.determineEntrypoints();
      } else {
        const resultR = this.r.determineEntrypoints();
        const resultL = this.l.determineEntrypoints();
        const combined = new Set<NodeIDX>([...resultL.set, ...resultR.set]);
        this.entrypointsCache = { vec: Array.from(combined), set: combined };
      }
    }
    return this.entrypointsCache;
  }

  getNodeName(nodeIDX: NodeIDX): string {
    // Node names are shared so it should not matter which graph we're
    // getting it from
    return node_idx_to_name(nodeIDX);
  }

  // This function is O(log(n)) because it uses a binary search.
  // We can use it for small to medium lookups, but it will get
  // pretty slow for very large lookups
  getNodeIDXByNameLog(name: string): NodeIDX | null {
    return node_name_to_idx_log(name) ?? null;
  }

  getArrowPairs(
    nodeIDX: NodeIDX,
    graph_structure: GraphStructureU8,
  ): ArrowPair[] {
    const arrowsJSON = get_arrow_pairs(nodeIDX, graph_structure);
    const parsed: Array<[Arrow | null, Arrow | null]> = JSON.parse(arrowsJSON);
    const result = [];
    for (const [arrowL, arrowR] of parsed) {
      if (arrowL != null && arrowR != null) {
        if (
          arrowL.points_to !== arrowR.points_to ||
          arrowL.points_from !== arrowR.points_from
        ) {
          throw new Error(
            `Inconsistent arrow points for node_idx: ${nodeIDX}. L: ${JSON.stringify(arrowL)} R: ${JSON.stringify(arrowR)}`,
          );
        }

        result.push({
          points_to: arrowL.points_to,
          points_from: arrowL.points_from,
          l: arrowL,
          r: arrowR,
        });
      } else if (arrowL != null) {
        result.push({
          points_to: arrowL.points_to,
          points_from: arrowL.points_from,
          l: arrowL,
          r: null,
        });
      } else if (arrowR != null) {
        result.push({
          points_to: arrowR.points_to,
          points_from: arrowR.points_from,
          l: null,
          r: arrowR,
        });
      } else {
        throw new Error(
          `Inconsistent arrow pairs for node_idx: ${nodeIDX}. Both arrows are null.`,
        );
      }
    }
    return result;
  }
}

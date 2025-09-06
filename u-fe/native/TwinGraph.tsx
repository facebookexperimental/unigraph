// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  get_arrow_pairs,
  get_transitive_count_delta,
  node_idx_to_name,
  node_name_to_idx_log,
  search_node_name_fuzzy,
} from "../../.build/wasm/unigraph_wasm";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import type { NodeIDX } from "../types";
import { SingleMetricsCache } from "./MetricCaches";
import type NativeGraph from "./NativeGraph";
import type { GraphStructureU8, NodeIDXVecSet } from "./NativeGraph";

/// This is a wrapper class over One or Two Native Graphs (left + ?right)
export default class TwinGraph {
  readonly l: NativeGraph;
  readonly r: NativeGraph | null;
  private entrypointsCache: NodeIDXVecSet | null = null;
  private transitiveCountDeltaCache: SingleMetricsCache;

  constructor(l: NativeGraph, r: NativeGraph | null) {
    this.l = l;
    this.r = r;
    this.transitiveCountDeltaCache = new SingleMetricsCache(
      l.nodeCount,
      (nodeIDXs: NodeIDX[]) =>
        new Float32Array(get_transitive_count_delta(new Uint32Array(nodeIDXs))),
    );
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

  search_nodes_fuzzy(pattern: string, limit: number): string[] {
    return search_node_name_fuzzy(pattern, limit);
  }

  getTransitiveCountDelta(nodeIDX: NodeIDX[]): Float32Array {
    return this.transitiveCountDeltaCache.getForIDXs(nodeIDX);
  }

  getArrowPairs(
    nodeIDX: NodeIDX,
    graph_structure: GraphStructureU8,
  ): TwinArrow[] {
    const arrowsJSON = get_arrow_pairs(nodeIDX, graph_structure, false);
    const parsed: TwinArrow[] = JSON.parse(arrowsJSON);
    return parsed;
  }
}

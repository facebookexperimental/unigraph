// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GraphStructure } from "@/__generated__/ts/GraphStructure";
import type { TraversalType } from "@/__generated__/ts/TraversalType";
import {
  get_arrow_pairs,
  get_graph_node_count,
  get_shortest_path,
  get_transitive_count_delta,
  get_transitive_tiered_metrics_delta,
  node_idx_to_name,
  node_name_to_idx_log,
  search_node_name_fuzzy,
} from "../../.build/wasm/unigraph_wasm";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import type { NodeIDX } from "../types";
import type { KeyedMetrics } from "./MetricCaches";
import { KeyedMetricsCache, SingleMetricsCache } from "./MetricCaches";
import type NativeGraph from "./NativeGraph";
import {
  GRAPH_SIDE,
  GRAPH_STRUCTURE,
  type GraphStructureU8,
  type NodeIDXVecSet,
  TRAVERSAL_TYPE,
} from "./NativeGraph";

/// This is a wrapper class over One or Two Native Graphs (left + ?right)
export default class TwinGraph {
  readonly nodeCount: number;
  readonly l: NativeGraph;
  readonly r: NativeGraph | null;
  private entrypointsCache: NodeIDXVecSet | null = null;
  private transitiveCountDeltaCache: SingleMetricsCache;
  private transitiveTieredDeltaCache: Map<string, KeyedMetricsCache>;

  constructor(l: NativeGraph, r: NativeGraph | null) {
    this.l = l;
    this.r = r;
    this.nodeCount = get_graph_node_count(GRAPH_SIDE.L);
    this.transitiveCountDeltaCache = new SingleMetricsCache(
      l.nodeCount,
      (nodeIDXs: NodeIDX[]) =>
        new Float32Array(get_transitive_count_delta(new Uint32Array(nodeIDXs))),
    );
    this.transitiveTieredDeltaCache = new Map<string, KeyedMetricsCache>();
  }

  isDeltaGraph(): boolean {
    return this.r != null;
  }

  rightGraphX(): NativeGraph {
    if (this.r == null) {
      throw new Error("twin graph does not have a graph on the right side.");
    }
    return this.r;
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

  getTwinArrows(
    nodeIDX: NodeIDX,
    graph_structure: GraphStructureU8,
    changedNodesOnly: boolean,
  ): TwinArrow[] {
    const arrowsJSON = get_arrow_pairs(
      nodeIDX,
      graph_structure,
      changedNodesOnly,
    );
    const parsed: TwinArrow[] = JSON.parse(arrowsJSON);
    return parsed;
  }

  getShortestPath(
    fromNodeIDX: readonly NodeIDX[],
    toNodeIDX: NodeIDX,
    graphStructure: GraphStructure,
    traversalType: TraversalType,
    changedNodesOnly: boolean,
  ): NodeIDX[] | null {
    const path = get_shortest_path(
      new Uint32Array(fromNodeIDX),
      toNodeIDX,
      graphStructureToU8(graphStructure),
      traversalTypeToU8(traversalType),
      changedNodesOnly,
    );

    if (path == null || path.length === 0) {
      return null;
    }

    return Array.from(path) as NodeIDX[];
  }

  getTieredTransitiveMetricsDeltaBatched(
    nodeIDXs: NodeIDX[],
    metricName: string,
  ): KeyedMetrics[] {
    return this.getOrInitForTransitiveTieredDelta(metricName).getForIDXs(
      nodeIDXs,
    );
  }

  private getOrInitForTransitiveTieredDelta(
    metricName: string,
  ): KeyedMetricsCache {
    if (this.transitiveTieredDeltaCache.has(metricName)) {
      return this.transitiveTieredDeltaCache.get(
        metricName,
      ) as KeyedMetricsCache;
    }
    const getMetrics = (nodeIDXs: NodeIDX[]) => {
      const metricsJSON = get_transitive_tiered_metrics_delta(
        new Uint32Array(nodeIDXs),
        metricName,
      );
      return JSON.parse(metricsJSON) as Array<KeyedMetrics>;
    };
    const metricsCache = new KeyedMetricsCache(this.nodeCount, getMetrics);
    this.transitiveTieredDeltaCache.set(metricName, metricsCache);
    return metricsCache;
  }
}

function graphStructureToU8(graphStructure: GraphStructure): number {
  switch (graphStructure) {
    case "Forward":
      return GRAPH_STRUCTURE.FORWARD;
    case "Dominator":
      return GRAPH_STRUCTURE.DOMINATOR;
    case "Reverse":
      return GRAPH_STRUCTURE.REVERSE;
    default: {
      const _exhaustiveCheck: never = graphStructure;
      throw new Error(`Unknown graph structure: ${_exhaustiveCheck}`);
    }
  }
}

function traversalTypeToU8(traversalType: TraversalType): number {
  switch (traversalType) {
    case "Configured":
      return TRAVERSAL_TYPE.CONFIGURED;
    case "Unconfigured":
      return TRAVERSAL_TYPE.UNCONFIGURED;
    default: {
      const _exhaustiveCheck: never = traversalType;
      throw new Error(`Unknown traversal type: ${_exhaustiveCheck}`);
    }
  }
}

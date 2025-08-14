// Copyright (c) Meta Platforms, Inc. and affiliates.

// This is a wrapper class over the state of the graph on the WASM side.

import {
  apply_traversal_config,
  determine_entrypoints,
  get_all_reachable_node_idxs,
  get_array_graph_stats,
  get_combined_metrics_for_entrypoints_with_force_include,
  get_combined_metrics_for_nodes,
  get_conjoint_cost,
  get_graph_node_count,
  get_graph_settings,
  get_graph_traversal_config,
  get_metric_names,
  get_node_metrics,
  get_reverse_edges_len,
  get_shortest_path,
  get_transitive_count,
  get_transitive_count_dominated,
  get_transitive_metrics,
  get_transitive_tiered_metrics,
  node_idx_to_name,
  node_name_to_idx_log,
  set_graphs,
} from "../.build/wasm/unigraph_wasm";
import type { ArrayGraphStats } from "./__generated__/ts/ArrayGraphStats";
import type { CombinedMetricsForNodes } from "./__generated__/ts/CombinedMetricsForNodes";
import type { ConjointCost } from "./__generated__/ts/ConjointCost";
import type { ExplorerComponentInputGraphs } from "./__generated__/ts/ExplorerComponentInputGraphs";
import type { GraphSettings } from "./__generated__/ts/GraphSettings";
import type { GraphStructure } from "./__generated__/ts/GraphStructure";
import type { TraversalConfig } from "./__generated__/ts/TraversalConfig";
import type { NodeIDX } from "./types";

export type GraphSide = 1 | 2;
export const GRAPH_SIDE = {
  L: 1 as const,
  R: 2 as const,
};

export type NodeIDXVecSet = Readonly<{
  vec: Readonly<NodeIDX[]>;
  set: Readonly<Set<NodeIDX>>;
}>;

type CombinedMetricsCache = {
  // metrics for the whole reachable graph from current entrypoints with
  // the current traversal config
  current: CombinedMetricsForNodes;
  // Same, but including edge overrides, which is force including an edge
  // and see how much it changes the metrics.
  // from -> to -> metrics
  for_overrides: Map<NodeIDX, Map<NodeIDX, CombinedMetricsForNodes>>;
};

export type GraphStructureU8 = 0 | 1 | 2;

export const GRAPH_STRUCTURE = {
  FORWARD: 0 as const,
  DOMINATOR: 1 as const,
  REVERSE: 2 as const,
};

// It serves as a bridge/cache layer between JS and WASM.
export default class NativeGraph {
  readonly nodeCount: number;
  readonly metricNames: string[] = [];
  readonly side: GraphSide;

  private metricCaches: MetricCaches;
  private statsCache: ArrayGraphStats | null = null;
  private parentsCountCache: SingleMetricsCache;
  private transitiveCountCache: SingleMetricsCache;
  private transitiveCountDominatedCache: SingleMetricsCache;
  private conjointCostCache: ConjointCost | null = null;

  private entrypointsCache: NodeIDXVecSet | null = null;

  private allReacahableNodeIDXsCache: NodeIDXVecSet | null = null;

  private combinedMetricsCache: CombinedMetricsCache | null = null;

  static fromSerialized(
    serialized: ExplorerComponentInputGraphs,
  ): [NativeGraph, NativeGraph | null] {
    set_graphs(JSON.stringify(serialized));
    if (serialized.right == null) {
      return [new NativeGraph(GRAPH_SIDE.L), null];
    } else {
      return [new NativeGraph(GRAPH_SIDE.L), new NativeGraph(GRAPH_SIDE.R)];
    }
  }

  /// Initializes a new graph on the WASM side and makes a new
  /// instance of this class.
  /// This should also act as a cache breaker for any state that's
  /// tied to the previous graph.
  constructor(side: GraphSide) {
    this.side = side;
    this.nodeCount = get_graph_node_count(this.side);
    this.metricNames = get_metric_names(this.side);
    this.metricCaches = new MetricCaches(this.nodeCount, side);
    this.parentsCountCache = new SingleMetricsCache(
      this.nodeCount,
      (nodeIDXs: NodeIDX[]) =>
        new Float32Array(
          get_reverse_edges_len(new Uint32Array(nodeIDXs), this.side),
        ),
    );
    this.transitiveCountCache = new SingleMetricsCache(
      this.nodeCount,
      (nodeIDXs: NodeIDX[]) =>
        new Float32Array(
          get_transitive_count(new Uint32Array(nodeIDXs), this.side),
        ),
    );
    this.transitiveCountDominatedCache = new SingleMetricsCache(
      this.nodeCount,
      (nodeIDXs: NodeIDX[]) =>
        new Float32Array(
          get_transitive_count_dominated(new Uint32Array(nodeIDXs), this.side),
        ),
    );
  }

  stats(): ArrayGraphStats {
    this.statsCache ??= JSON.parse(get_array_graph_stats(this.side));
    return this.statsCache as ArrayGraphStats;
  }

  /// Get the current traversal config that's set on the grpaph.
  /// This will usually return the default traversal config encoded
  /// on the graph if nothing else was explicitly set.
  getTraversalConfig(): TraversalConfig {
    const tvcJSON = get_graph_traversal_config(this.side);
    return JSON.parse(tvcJSON) as TraversalConfig;
  }

  getGraphSettings(): GraphSettings {
    const graphSettingsJSON = get_graph_settings(this.side);
    return JSON.parse(graphSettingsJSON) as GraphSettings;
  }

  /// This function changes the traversal config and returns a new
  /// reference to the graph. All caches should be nuked.
  getApplyTraversalConfig(traversalConfig: TraversalConfig): NativeGraph {
    apply_traversal_config(JSON.stringify(traversalConfig), this.side);
    return new NativeGraph(this.side);
  }

  getNodeName(nodeIDX: NodeIDX): string {
    return node_idx_to_name(nodeIDX);
  }

  // This function is O(log(n)) because it uses a binary search.
  // We can use it for small to medium lookups, but it will get
  // pretty slow for very large lookups
  getNodeIDXByNameLog(name: string): NodeIDX | null {
    return node_name_to_idx_log(name) ?? null;
  }

  getShortestPath(
    fromNodeIDX: readonly NodeIDX[],
    toNodeIDX: NodeIDX,
    graphStructure: GraphStructure,
  ): NodeIDX[] | null {
    const path = get_shortest_path(
      new Uint32Array(fromNodeIDX),
      toNodeIDX,
      graphStructureToU8(graphStructure),
      this.side,
    );

    if (path == null || path.length === 0) {
      return null;
    }

    return Array.from(path) as NodeIDX[];
  }

  determineEntrypoints(): NodeIDXVecSet {
    if (this.entrypointsCache == null) {
      const result = determine_entrypoints(this.side);
      this.entrypointsCache = { vec: Array.from(result), set: new Set(result) };
    }
    return this.entrypointsCache;
  }

  getAllReachableNodeIDXs(): NodeIDXVecSet {
    if (this.allReacahableNodeIDXsCache == null) {
      const result = get_all_reachable_node_idxs(this.side);
      this.allReacahableNodeIDXsCache = {
        vec: Array.from(result),
        set: new Set(result),
      };
    }
    return this.allReacahableNodeIDXsCache;
  }

  isNodeReachable(nodeIDX: NodeIDX): boolean {
    return this.getAllReachableNodeIDXs().set.has(nodeIDX);
  }

  getNodeMetric(nodeIDX: NodeIDX, metricName: string): number {
    const value: number | undefined = this.metricCaches
      .getOrInitForPlain(metricName)
      .getForIDXs([nodeIDX])[0];
    return value as number;
  }

  getNodeMetricBatched(nodeIDXs: NodeIDX[], metricName: string): Float32Array {
    return this.metricCaches.getOrInitForPlain(metricName).getForIDXs(nodeIDXs);
  }

  getParentsCount(nodeIDX: NodeIDX[]): Float32Array {
    return this.parentsCountCache.getForIDXs(nodeIDX);
  }

  getTransitiveCount(nodeIDX: NodeIDX[]): Float32Array {
    return this.transitiveCountCache.getForIDXs(nodeIDX);
  }

  getTransitiveCountDominated(nodeIDX: NodeIDX[]): Float32Array {
    return this.transitiveCountDominatedCache.getForIDXs(nodeIDX);
  }

  getTransitiveMetric(nodeIDX: NodeIDX, metricName: string): number {
    const value: number | undefined = this.metricCaches
      .getOrInitForTransitive(metricName)
      .getForIDXs([nodeIDX])[0];
    return value as number;
  }

  getTransitiveMetricsBatched(
    nodeIDXs: NodeIDX[],
    metricName: string,
  ): Float32Array {
    return this.metricCaches
      .getOrInitForTransitive(metricName)
      .getForIDXs(nodeIDXs);
  }

  getTransitiveDominatedMetricsBatched(
    nodeIDXs: NodeIDX[],
    metricName: string,
  ): Float32Array {
    return this.metricCaches
      .getOrInitForTransitiveDominated(metricName)
      .getForIDXs(nodeIDXs);
  }

  getTieredTransitiveMetric(
    nodeIDX: NodeIDX,
    metricName: string,
  ): KeyedMetrics {
    const value = this.metricCaches
      .getOrInitForTransitiveTiered(metricName)
      .getForIDXs([nodeIDX])[0];
    return value as KeyedMetrics;
  }

  getTieredTransitiveMetricsBatched(
    nodeIDXs: NodeIDX[],
    metricName: string,
  ): KeyedMetrics[] {
    return this.metricCaches
      .getOrInitForTransitiveTiered(metricName)
      .getForIDXs(nodeIDXs);
  }

  getTieredTransitiveMetricsDominatedBatched(
    nodeIDXs: NodeIDX[],
    metricName: string,
  ): KeyedMetrics[] {
    return this.metricCaches
      .getOrInitForTransitiveTieredDominated(metricName)
      .getForIDXs(nodeIDXs);
  }

  getCombinedMetrics(nodeIDXs: NodeIDX[]): CombinedMetricsForNodes {
    const json = get_combined_metrics_for_nodes(
      new Uint32Array(nodeIDXs),
      this.side,
    );
    return JSON.parse(json) as CombinedMetricsForNodes;
  }

  private getOrInitCombinedMetricsCache(): CombinedMetricsCache {
    if (this.combinedMetricsCache == null) {
      const currentJSON =
        get_combined_metrics_for_entrypoints_with_force_include(
          null,
          null,
          this.side,
        );

      const current: CombinedMetricsForNodes = JSON.parse(currentJSON);
      this.combinedMetricsCache = {
        current,
        for_overrides: new Map(),
      };
    }

    return this.combinedMetricsCache;
  }

  getCombinedMetricsForEntryPoints(): CombinedMetricsForNodes {
    return this.getOrInitCombinedMetricsCache().current;
  }

  getCombinedMetricsForEntryPointsWithOverrides(forceInclude: {
    from: NodeIDX;
    to: NodeIDX;
  }): CombinedMetricsForNodes {
    const cache = this.getOrInitCombinedMetricsCache();

    let result = cache.for_overrides
      .get(forceInclude.from)
      ?.get(forceInclude.to);

    if (result != null) {
      return result;
    }

    const metricsJSON = get_combined_metrics_for_entrypoints_with_force_include(
      forceInclude.from,
      forceInclude.to,
      this.side,
    );
    result = JSON.parse(metricsJSON) as CombinedMetricsForNodes;

    const fromMap = cache.for_overrides.get(forceInclude.from) ?? new Map();
    fromMap.set(forceInclude.to, result);
    cache.for_overrides.set(forceInclude.from, fromMap);
    return result;
  }

  getConjointCost(): ConjointCost {
    if (this.conjointCostCache == null) {
      const json = get_conjoint_cost(this.side);
      this.conjointCostCache = JSON.parse(json) as ConjointCost;
    }
    return this.conjointCostCache;
  }
}

class MetricCaches {
  // plain metrics come directly from the graph's nodes.
  // This cache will basically just copy the metrics to the JS
  // side as is and we'll use it to avoid crossing the boundary
  private node_metrics: Map<string, SingleMetricsCache>;
  private transitive: Map<string, SingleMetricsCache>;
  private transitive_dominated: Map<string, SingleMetricsCache>;
  private transitive_tiered: Map<string, KeyedMetricsCache>;
  private transitive_tiered_dominated: Map<string, KeyedMetricsCache>;
  private side: GraphSide;

  constructor(
    private nodeCount: number,
    side: GraphSide,
  ) {
    this.side = side;
    this.node_metrics = new Map<string, SingleMetricsCache>();
    this.transitive = new Map<string, SingleMetricsCache>();
    this.transitive_dominated = new Map<string, SingleMetricsCache>();
    this.transitive_tiered = new Map<string, KeyedMetricsCache>();
    this.transitive_tiered_dominated = new Map<string, KeyedMetricsCache>();
  }

  getOrInitForTransitive(metricName: string): SingleMetricsCache {
    if (this.transitive.has(metricName)) {
      return this.transitive.get(metricName) as SingleMetricsCache;
    }
    const getMetrics = (nodeIDXs: NodeIDX[]) =>
      get_transitive_metrics(
        new Uint32Array(nodeIDXs),
        metricName,
        false,
        this.side,
      );

    const metricsCache = new SingleMetricsCache(this.nodeCount, getMetrics);
    this.transitive.set(metricName, metricsCache);
    return metricsCache;
  }

  getOrInitForTransitiveDominated(metricName: string): SingleMetricsCache {
    if (this.transitive_dominated.has(metricName)) {
      return this.transitive_dominated.get(metricName) as SingleMetricsCache;
    }
    const getMetrics = (nodeIDXs: NodeIDX[]) =>
      get_transitive_metrics(
        new Uint32Array(nodeIDXs),
        metricName,
        true,
        this.side,
      );

    const metricsCache = new SingleMetricsCache(this.nodeCount, getMetrics);
    this.transitive_dominated.set(metricName, metricsCache);
    return metricsCache;
  }

  getOrInitForPlain(metricName: string): SingleMetricsCache {
    if (this.node_metrics.has(metricName)) {
      return this.node_metrics.get(metricName) as SingleMetricsCache;
    }
    const getMetrics = (nodeIDXs: NodeIDX[]) =>
      get_node_metrics(new Uint32Array(nodeIDXs), metricName, this.side);

    const metricsCache = new SingleMetricsCache(this.nodeCount, getMetrics);
    this.node_metrics.set(metricName, metricsCache);
    return metricsCache;
  }

  getOrInitForTransitiveTiered(metricName: string): KeyedMetricsCache {
    if (this.transitive_tiered.has(metricName)) {
      return this.transitive_tiered.get(metricName) as KeyedMetricsCache;
    }
    const getMetrics = (nodeIDXs: NodeIDX[]) => {
      const metricsJSON = get_transitive_tiered_metrics(
        new Uint32Array(nodeIDXs),
        metricName,
        false,
        this.side,
      );
      return JSON.parse(metricsJSON) as Array<KeyedMetrics>;
    };
    const metricsCache = new KeyedMetricsCache(this.nodeCount, getMetrics);
    this.transitive_tiered.set(metricName, metricsCache);
    return metricsCache;
  }

  getOrInitForTransitiveTieredDominated(metricName: string): KeyedMetricsCache {
    if (this.transitive_tiered_dominated.has(metricName)) {
      return this.transitive_tiered_dominated.get(
        metricName,
      ) as KeyedMetricsCache;
    }
    const getMetrics = (nodeIDXs: NodeIDX[]) => {
      const metricsJSON = get_transitive_tiered_metrics(
        new Uint32Array(nodeIDXs),
        metricName,
        true,
        this.side,
      );
      return JSON.parse(metricsJSON) as Array<KeyedMetrics>;
    };
    const metricsCache = new KeyedMetricsCache(this.nodeCount, getMetrics);
    this.transitive_tiered_dominated.set(metricName, metricsCache);
    return metricsCache;
  }
}

/// Class that contains metrics extracted from the graph on WASM side.
/// WASM<->JS interop is not cheap, so we do these operations in
/// batches and cache the results on JS side so we don't have to go
/// multiple to WASM for the same data.
class SingleMetricsCache {
  private metrics: Float32Array;
  private valueExists: Uint8Array;
  private getMetrics: (nodeIDXs: NodeIDX[]) => Float32Array;

  constructor(size: number, getMetrics: (nodeIDXs: NodeIDX[]) => Float32Array) {
    this.metrics = new Float32Array(size);
    this.valueExists = new Uint8Array(size).fill(0);
    this.getMetrics = getMetrics;
  }

  getForIDXs(nodeIDXs: NodeIDX[]): Float32Array {
    const cacheMissesIDXs: NodeIDX[] = [];

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      if (this.valueExists[nodeIDX] === 0) {
        cacheMissesIDXs.push(nodeIDX);
      }
    }

    if (cacheMissesIDXs.length !== 0) {
      const newMetrics = this.getMetrics(cacheMissesIDXs);
      for (let i = 0; i < cacheMissesIDXs.length; i++) {
        const nodeIDX = cacheMissesIDXs[i] as NodeIDX;
        const value = newMetrics[i] as number;
        this.metrics[nodeIDX] = value;
        this.valueExists[nodeIDX] = 1;
      }
    }

    const result = new Float32Array(nodeIDXs.length);

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      // at this point there should not be missing values
      result[i] = this.metrics[nodeIDX] as number;
    }

    return result;
  }
}

export type KeyedMetrics = { [metricName: string]: number };
class KeyedMetricsCache {
  private metrics: Array<KeyedMetrics | null>;
  private getMetrics: (nodeIDXs: NodeIDX[]) => Array<KeyedMetrics>;

  constructor(
    size: number,
    getMetrics: (nodeIDXs: NodeIDX[]) => Array<KeyedMetrics>,
  ) {
    this.metrics = new Array(size).fill(null);
    this.getMetrics = getMetrics;
  }

  getForIDXs(nodeIDXs: NodeIDX[]): Array<KeyedMetrics> {
    const cacheMissesIDXs: NodeIDX[] = [];

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      if (this.metrics[nodeIDX] === null) {
        cacheMissesIDXs.push(nodeIDX);
      }
    }

    if (cacheMissesIDXs.length !== 0) {
      const newMetrics = this.getMetrics(cacheMissesIDXs);
      for (let i = 0; i < cacheMissesIDXs.length; i++) {
        const nodeIDX = cacheMissesIDXs[i] as NodeIDX;
        const value = newMetrics[i] as KeyedMetrics;
        this.metrics[nodeIDX] = value;
      }
    }

    const result = [];

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      // at this point there should not be missing values
      result[i] = this.metrics[nodeIDX] as KeyedMetrics;
    }

    return result;
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

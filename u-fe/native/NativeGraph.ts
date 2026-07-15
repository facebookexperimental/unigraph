// Copyright (c) Meta Platforms, Inc. and affiliates.

// This is a wrapper class over the state of the graph on the WASM side.

import {
  apply_traversal_config,
  available_metric_views,
  determine_entrypoints,
  get_array_graph_stats,
  get_combined_metrics_for_entrypoints_with_force_include,
  get_combined_metrics_for_nodes,
  get_graph_node_count,
  get_graph_settings,
  get_graph_traversal_config,
  get_metric_min_max,
  get_metric_names,
  get_node_flags,
  get_node_metrics,
  get_reverse_edges_len,
  get_transitive_count,
  get_transitive_count_dominated,
  get_transitive_metrics,
  get_transitive_tiered_metrics,
  node_idx_to_name,
  node_name_to_idx_log,
  set_graph_settings,
  set_graphs,
  visible_metric_views,
} from "../../.build/wasm/unigraph_wasm";
import type { ArrayGraphStats } from "../__generated__/ts/ArrayGraphStats";
import type { CombinedMetricsForNodes } from "../__generated__/ts/CombinedMetricsForNodes";
import type { ExplorerComponentInputGraphs } from "../Explorer";
import type { GraphSettings } from "../__generated__/ts/GraphSettings";
import type { TraversalConfig } from "../__generated__/ts/TraversalConfig";
import type { NodeIDX } from "../types";
import {
  type KeyedMetrics,
  KeyedMetricsCache,
  SingleMetricsCache,
} from "./MetricCaches";
import { isNodeUnreachable, type TierIDX, tierIdx } from "./NodeFlags";

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

export const TRAVERSAL_TYPE = {
  CONFIGURED: 0 as const,
  UNCONFIGURED: 1 as const,
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

  private entrypointsCache: NodeIDXVecSet | null = null;

  private nodeFlagsCache: Uint32Array | null = null;
  private allReachableNodeIDXsCache: NodeIDXVecSet | null = null;

  private combinedMetricsCache: CombinedMetricsCache | null = null;

  // Memoized min/max of a metric across ALL nodes, keyed by metric name.
  // `null` means the metric is absent/empty.
  private metricMinMaxCache = new Map<
    string,
    { min: number; max: number } | null
  >();

  static fromSerialized(
    serialized: ExplorerComponentInputGraphs,
  ): [NativeGraph | null, NativeGraph] {
    set_graphs(JSON.stringify(serialized));
    if (serialized.left == null) {
      return [null, new NativeGraph(GRAPH_SIDE.R)];
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

  setGraphSettings(gs: GraphSettings): void {
    set_graph_settings(JSON.stringify(gs), this.side);
  }

  getAvailableMetricViews(): string[] {
    return JSON.parse(available_metric_views(this.side)) as string[];
  }

  getVisibleMetricViews(structure: number): string[] {
    return JSON.parse(visible_metric_views(this.side, structure)) as string[];
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

  determineEntrypoints(): NodeIDXVecSet {
    if (this.entrypointsCache == null) {
      const result = determine_entrypoints(this.side);
      this.entrypointsCache = { vec: Array.from(result), set: new Set(result) };
    }
    return this.entrypointsCache;
  }

  getAllReachableNodeIDXs(): NodeIDXVecSet {
    if (this.allReachableNodeIDXsCache == null) {
      const flags = this.getOrInitNodeFlagsCache();
      const allReachable = [];
      for (let i = 0; i < flags.length; i++) {
        const bits = flags[i] as number;
        if (isNodeUnreachable(bits)) {
          continue;
        }
        allReachable.push(i as NodeIDX);
      }
      this.allReachableNodeIDXsCache = {
        vec: Array.from(allReachable),
        set: new Set(allReachable),
      };
    }
    return this.allReachableNodeIDXsCache;
  }

  getNodeTierIDX(nodeIDX: NodeIDX): TierIDX | null {
    const flags = this.getOrInitNodeFlagsCache();
    const bits = flags[nodeIDX] as number;
    return tierIdx(bits);
  }

  getNodeTierName(nodeIDX: NodeIDX): [string, TierIDX] | null {
    const tierIdx = this.getNodeTierIDX(nodeIDX);
    if (tierIdx == null) return null;
    const tierName = this.stats().tier_names[tierIdx];
    if (tierName == null) return null;
    return [tierName, tierIdx];
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

  /// Min and max of a metric across ALL nodes (reachable or not), computed
  /// once in Rust and memoized here. `null` when the metric is absent/empty.
  /// When `ignoreZero` is set, `0.0` values (the default for missing metrics)
  /// are excluded from the range. Memoized per (metric, ignoreZero) so we
  /// never recompute it per row.
  getMetricMinMax(
    metricName: string,
    ignoreZero: boolean = false,
  ): { min: number; max: number } | null {
    const cacheKey = `${metricName} ${ignoreZero}`;
    const cached = this.metricMinMaxCache.get(cacheKey);
    if (cached !== undefined) {
      return cached;
    }
    const result = get_metric_min_max(metricName, ignoreZero, this.side);
    const [min, max] = result;
    const value = min !== undefined && max !== undefined ? { min, max } : null;
    this.metricMinMaxCache.set(cacheKey, value);
    return value;
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

  private getOrInitNodeFlagsCache(): Uint32Array {
    if (this.nodeFlagsCache == null) {
      this.nodeFlagsCache = get_node_flags(this.side);
    }
    return this.nodeFlagsCache;
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

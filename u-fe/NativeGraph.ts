// Copyright (c) Meta Platforms, Inc. and affiliates.

// This is a wrapper class over the state of the graph on the WASM side.

import {
  apply_traversal_config,
  determine_entrypoints,
  get_arrows_forward,
  get_graph_node_count,
  get_metric_names,
  get_node_metrics,
  get_transitive_metrics,
  node_idx_to_name,
  node_name_to_idx_log,
  set_graph,
} from "../.build/wasm/unigraph_wasm";
import type { NodeIDX } from "./types";
import type { Arrow } from "../u-be/unigraph_core/bindings/Arrow";
import type { TraversalConfig } from "u-be/unigraph_core/bindings/TraversalConfig";

// It serves as a bridge/cache layer between JS and WASM.
export default class NativeGraph {
  private entrypoints: NodeIDX[] | null = null;

  readonly nodeCount: number;
  readonly metricNames: string[] = [];
  private metricCaches: MetricCaches;

  static fromMapGraphJSON(mapGraphJSON: string): NativeGraph {
    set_graph(mapGraphJSON);
    return new NativeGraph();
  }

  /// Initializes a new graph on the WASM side and makes a new
  /// instance of this class.
  /// This should also act as a cache breaker for any state that's
  /// tied to the previous graph.
  constructor() {
    this.nodeCount = get_graph_node_count();
    this.entrypoints = null;
    this.metricNames = get_metric_names();
    this.metricCaches = new MetricCaches(this.nodeCount);
  }

  /// This function changes the traversal config and returns a new
  /// reference to the graph. All caches should be nuked.
  getApplyTraversalConfig(traversalConfig: TraversalConfig): NativeGraph {
    apply_traversal_config(JSON.stringify(traversalConfig));
    return new NativeGraph();
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

  getArrowsForward(nodeIDX: NodeIDX): Arrow[] {
    const arrowsJSON = get_arrows_forward(nodeIDX);
    const parsed = JSON.parse(arrowsJSON);
    return parsed as Arrow[];
  }

  determineEntrypoints(): NodeIDX[] {
    if (this.entrypoints == null) {
      this.entrypoints = Array.from(determine_entrypoints());
    }
    return this.entrypoints;
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
}

class MetricCaches {
  // plain metrics come directly from the graph's nodes.
  // This cache will basically just copy the metrics to the JS
  // side as is and we'll use it to avoid crossing the boundary
  private node_metrics: Map<string, MetricsCache>;
  private transitive: Map<string, MetricsCache>;

  constructor(private nodeCount: number) {
    this.node_metrics = new Map<string, MetricsCache>();
    this.transitive = new Map<string, MetricsCache>();
  }

  getOrInitForTransitive(metricName: string): MetricsCache {
    if (this.transitive.has(metricName)) {
      return this.transitive.get(metricName) as MetricsCache;
    }
    const getMetics = (nodeIDXs: NodeIDX[]) =>
      get_transitive_metrics(new Uint32Array(nodeIDXs), metricName);

    const metricsCache = new MetricsCache(this.nodeCount, getMetics);
    this.transitive.set(metricName, metricsCache);
    return metricsCache;
  }

  getOrInitForPlain(metricName: string): MetricsCache {
    if (this.node_metrics.has(metricName)) {
      return this.node_metrics.get(metricName) as MetricsCache;
    }
    const getMetics = (nodeIDXs: NodeIDX[]) =>
      get_node_metrics(new Uint32Array(nodeIDXs), metricName);

    const metricsCache = new MetricsCache(this.nodeCount, getMetics);
    this.node_metrics.set(metricName, metricsCache);
    return metricsCache;
  }
}

/// Class that contains metrics extracted from the graph on WASM side.
/// WASM<->JS interop is not cheap, so we do these operations in
/// batches and cache the results on JS side so we don't have to go
/// multiple to WASM for the same data.
class MetricsCache {
  private metrics: Float32Array;
  private valueExists: Uint8Array;
  private getMetics: (nodeIDXs: NodeIDX[]) => Float32Array;

  constructor(size: number, getMetics: (nodeIDXs: NodeIDX[]) => Float32Array) {
    this.metrics = new Float32Array(size);
    this.valueExists = new Uint8Array(size).fill(0);
    this.getMetics = getMetics;
  }

  getForIDXs(nodeIDXs: NodeIDX[]): Float32Array {
    const result = new Float32Array(nodeIDXs.length);
    const cacheMisses: NodeIDX[] = [];
    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      if (this.valueExists[nodeIDX] === 0) {
        cacheMisses.push(nodeIDX);
      } else {
        result[i] = this.metrics[nodeIDX] as number;
      }
    }
    if (cacheMisses.length > 0) {
      const newMetrics = this.getMetics(cacheMisses);
      for (let i = 0; i < cacheMisses.length; i++) {
        const nodeIDX = cacheMisses[i] as NodeIDX;
        const value = newMetrics[i] as number;
        this.metrics[nodeIDX] = value;
        this.valueExists[nodeIDX] = 1;
        result[i] = value;
      }
    }

    return result;
  }
}

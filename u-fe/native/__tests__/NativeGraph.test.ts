// Copyright (c) Meta Platforms, Inc. and affiliates.

import { describe, expect, it, vi } from "vitest";

vi.mock("../../../.build/wasm/unigraph_wasm", () => ({
  get_graph_node_count: () => 3,
  get_metric_names: () => [],
  get_reverse_edges_len: vi.fn((idxs: Uint32Array) =>
    Float64Array.from(idxs, (idx) => idx + 1),
  ),
  get_transitive_count: vi.fn((idxs: Uint32Array) =>
    Float64Array.from(idxs, (idx) => idx + 2),
  ),
  get_transitive_count_dominated: vi.fn((idxs: Uint32Array) =>
    Float64Array.from(idxs, (idx) => idx + 3),
  ),
  apply_traversal_config: vi.fn(),
}));

vi.mock("../MetricCaches", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../MetricCaches")>();
  return {
    ...actual,
    SingleMetricsCache: vi.fn(function (
      size: number,
      getter: (idxs: number[]) => Float64Array,
    ) {
      return new actual.SingleMetricsCache(size, getter);
    }),
  };
});

import { SingleMetricsCache } from "../MetricCaches";
import NativeGraph, { GRAPH_SIDE } from "../NativeGraph";
import { get_reverse_edges_len } from "../../../.build/wasm/unigraph_wasm";

describe("NativeGraph count caches", () => {
  it("allocates only requested counts and reuses each cache", () => {
    vi.clearAllMocks();
    const graph = new NativeGraph(GRAPH_SIDE.R);
    expect(SingleMetricsCache).not.toHaveBeenCalled();
    expect(Array.from(graph.getParentsCount([0, 1]))).toEqual([1, 2]);
    expect(Array.from(graph.getParentsCount([1]))).toEqual([2]);
    expect(SingleMetricsCache).toHaveBeenCalledTimes(1);
    expect(get_reverse_edges_len).toHaveBeenCalledTimes(1);
    expect(Array.from(graph.getTransitiveCount([1]))).toEqual([3]);
    expect(Array.from(graph.getTransitiveCountDominated([1]))).toEqual([4]);
    expect(SingleMetricsCache).toHaveBeenCalledTimes(3);
  });

  it("starts new caches when traversal changes", () => {
    vi.clearAllMocks();
    const graph = new NativeGraph(GRAPH_SIDE.R);
    graph.getParentsCount([1]);
    const next = graph.getApplyTraversalConfig({});
    expect(SingleMetricsCache).toHaveBeenCalledTimes(1);
    expect(Array.from(next.getParentsCount([1]))).toEqual([2]);
    expect(SingleMetricsCache).toHaveBeenCalledTimes(2);
    expect(get_reverse_edges_len).toHaveBeenCalledTimes(2);
  });
});

/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<5998cf3fdf6eb5c3a87d12dc4db596d0>>
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { MapGraph } from './MapGraph.ts';
import type { PerfStats } from './PerfStats.ts';

/**
 * Like [`GraphQueryOutput`](super::GraphQueryOutput), but returns the graph as a
 * plain [`MapGraph`] instead of a packed `ArrayGraph`.
 * 
 * Meant for smaller queries and tiny graphs that don't need the `ArrayGraph`
 * CSR packing/compression — the caller gets a human-readable, directly
 * serializable graph back.
 */
export interface GraphQueryMapGraphOutput {
  map_graph: MapGraph;
  graph_query_config: GraphQueryConfig;
  /**
   * The resolved graph key of the snapshot this query landed on, formatted as
   * `"{timeline}~{graph_id}"` (e.g. `"www-budget~223"`). Unlike
   * `graph_query_config.handle` — which merely echoes the input handle — this
   * always carries the concrete `graph_id`, even when a bare (latest) handle
   * was sent. Lets clients pin follow-up links to the exact version rendered.
   */
  graph_key: string;
  /**
   * Where this request spent its time. `None` from a server predating the
   * field — absent rather than zeroed, so it is never mistaken for a
   * measurement.
   */
  perf_stats?: PerfStats | undefined;
}
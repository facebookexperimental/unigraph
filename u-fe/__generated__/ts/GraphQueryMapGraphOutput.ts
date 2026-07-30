/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0f11f05a001c0e64ef8a169aa5563fd6>>
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { MapGraph } from './MapGraph.ts';

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
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<d79212eb85953d51aeb8e4f129a3df37>>
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
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { NodeIDX } from './NodeIDX.ts';

/**
 * Serializable edge data for an array graph.
 * 
 * ALL edges (directed, tagged, dynamic) are stored in a single CSR layout.
 * Tagged/dynamic edges have entries in `edge_metadata` + `edge_metadata_map`
 * that describe their type and properties. Directed edges have no metadata entry.
 * 
 * This design enables zero-cost conversion to ArrayGraph — just move the data
 * and allocate runtime flags.
 */
export interface ArrayGraphSerializableEdges {
  /**
   * CSR targets for ALL edges (directed + tagged + dynamic).
   * Within each node's range: directed edges first, then tagged (sorted by tag + target),
   * then dynamic (sorted by type_key + edge_name + branch + target).
   */
  edges: NodeIDX[];
  /** CSR offsets: `edges[edge_offsets[i]..edge_offsets[i+1]]` gives targets for source node `i`. */
  edge_offsets: number[];
}
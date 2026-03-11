/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphDynamicEdge } from './ArrayGraphDynamicEdge.ts';
import type { NodeIDX } from './NodeIDX.ts';

/**
 * Serializable edge data for an array graph.
 * 
 * Directed edges are stored in a CSR (Compressed Sparse Row) layout:
 * `directed` is a flat list of target node indices and `directed_offsets`
 * provides per-source-node boundaries into that list.
 * 
 * Tagged and dynamic edges use map-based representations since they carry
 * additional metadata (tags, branch labels, properties).
 * 
 * Note: when serialized, only "pure" directed edges are included in the CSR
 * arrays — tagged and dynamic edges are excluded to avoid duplication, since
 * they are stored separately. On deserialization the full offset graph is
 * reconstructed by merging all three edge types back together.
 */
export interface ArrayGraphSerializableEdges {
  /** Flat list of directed-edge target node indices. */
  directed: NodeIDX[];
  /**
   * CSR offsets into `directed` — `directed[directed_offsets[i]..directed_offsets[i+1]]`
   * gives the targets for source node `i`.
   */
  directed_offsets: number[];
  /** Tagged edges: source node → tag → set of target nodes. */
  tagged: { [key: NodeIDX]: { [key: string]: NodeIDX[] } };
  /** Dynamic edges with runtime-defined branches and metadata. */
  dynamic: { [key: NodeIDX]: { [key: string]: { [key: string]: ArrayGraphDynamicEdge } } };
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { NodeIDX } from './NodeIDX.ts';

/**
 * This is a more heavyweight struct describing an edge in the graph.
 * This can represent any edge (directed/tagged/dynamic).
 * This is meant to be used for more sparce operations, like rendering
 * edges in the UI, or for debugging. Since these are much heavier they
 * are not fit for heavy computations, like DFS/BFS, computing transitive
 * metrics, etc.
 */
export interface Arrow {
  tag?: string | undefined;
  branch?: string | undefined;
  properties?: { [key: string]: string } | undefined;
  points_from: NodeIDX;
  points_to: NodeIDX;
  excluded: boolean;
  message?: string | undefined;
  /**
   * Relevant only for cases where arrows represent compressed path.
   * e.g. when we show Changed Nodes only each row in the tree table
   * represent a path from one node to another with potential nodes
   * in between skipped. This value will represent how many nodes
   * were skipped (shortest path)
   * 0 means direct edge.
   * 
   * Example:
   * 
   * Actual Graph:
   *         A
   *       /  \
   *      B    C
   *       \  /
   *         D     <- only changed node
   * 
   * Graph with changed nodes only:
   *         A
   *         |
   *         D     <- only changed node
   * 
   * The arrow will look like: { from: A, to: D, skipped: 1 }
   * where `1` means that D is at least 1 skippe node away from A
   */
  skipped: number;
}
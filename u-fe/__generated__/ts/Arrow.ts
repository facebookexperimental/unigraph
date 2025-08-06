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
}
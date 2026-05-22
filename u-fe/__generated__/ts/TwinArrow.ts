/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Arrow } from './Arrow.ts';
import type { NodeIDX } from './NodeIDX.ts';

/**
 * Matched arrows pair represents either a single arrow if we have a single graph
 * or two optional arrows if we're comparing two graphs.
 * There should not be a situation where we have both arrows null.
 * 
 * `points_to` and `points_from` are in the merged (TwinGraph) namespace.
 */
export interface TwinArrow {
  points_to: NodeIDX;
  points_from: NodeIDX;
  node_diff: number;
  l?: Arrow | undefined;
  r?: Arrow | undefined;
}
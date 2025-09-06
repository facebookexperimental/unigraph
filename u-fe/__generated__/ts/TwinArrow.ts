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
 * if we have two arrows they must BOTH point TO and FROM the same node
 */
export interface TwinArrow {
  points_to: NodeIDX;
  points_from: NodeIDX;
  node_diff: number;
  l?: Arrow | undefined;
  r?: Arrow | undefined;
}
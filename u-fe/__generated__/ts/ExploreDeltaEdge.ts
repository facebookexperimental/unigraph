/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f67f3d454d40a35f491fc451e6791d54>>
 */


import type { DynamicEdgeInfo } from './DynamicEdgeInfo.ts';

/**
 * The edge leading to a node, as it exists on one side of the twin graph.
 * 
 * Kept per-side rather than collapsed into the row: an edge can be *retagged*
 * (`lazy` -> `deferred`) or have its dynamic branch changed without either
 * endpoint node changing, and a single flattened `tag` would silently show
 * only the new value. Tags drive tiered traversal, so a retag is a real
 * behavioural change, not cosmetics.
 */
export interface ExploreDeltaEdge {
  /** Edge tag (e.g. "lazy"), if this is a tagged edge. */
  tag?: string | undefined;
  /** Dynamic edge info, if this is a dynamic edge. */
  dynamic?: DynamicEdgeInfo | undefined;
}
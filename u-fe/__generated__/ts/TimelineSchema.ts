/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AdjacentDeltasConfig } from './AdjacentDeltasConfig.ts';

/**
 * Schema that governs how frames in a timeline relate to each other.
 * 
 * Currently only [`AdjacentDeltas`](AdjacentDeltasConfig) is supported:
 * deltas must reference the immediately preceding frame as their base.
 */
export type TimelineSchema =
  /** Linear history where deltas derive from the immediately preceding graph. */
  { "AdjacentDeltas": AdjacentDeltasConfig };

export type TimelineSchemaVariants = "AdjacentDeltas";
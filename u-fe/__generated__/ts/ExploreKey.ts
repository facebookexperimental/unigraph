/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { TraversalOverride } from './TraversalOverride.ts';

/**
 * Composite key for the explore cache.
 * 
 * - `handle` — which graph: a timeline ID (`"my_timeline"`), graph key
 *   (`"my_timeline~123"`), or GQC key (`"gqc_abc123"`).
 * - `roots` — if present, overrides the handle's roots (GQC roots or
 *   graph entry points).
 * - `traversal` — if present, overrides the handle's traversal config.
 */
export interface ExploreKey {
  handle: string;
  roots?: string[] | undefined;
  traversal?: TraversalOverride | undefined;
}

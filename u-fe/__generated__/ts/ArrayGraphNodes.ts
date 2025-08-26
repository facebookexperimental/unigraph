/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

/**
 * Ordered list of all node names in a graph.
 * Stored as a massive single string with offsets recorded for how
 * to find each node. The single string is here so we don't have
 * to much memory fragmentation and can allocate, deallocate the
 * whole thing in one chunk. Searching though a single string is
 * also faster because we can optimize for CPU cache hits and
 * SIMD instructions.
 */
export interface ArrayGraphNodes {
  node_names: string;
  offsets: number[];
}

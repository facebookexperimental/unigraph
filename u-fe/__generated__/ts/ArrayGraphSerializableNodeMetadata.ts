/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { NodeIDX } from './NodeIDX.ts';

/** Serializable per-node metadata: numeric metrics and categorical tag sets. */
export interface ArrayGraphSerializableNodeMetadata {
  /**
   * Named metrics — each entry maps a metric name to a `Vec<f32>` with one
   * value per node (indexed by [`NodeIDX`]).
   */
  metrics: { [key: string]: number[] };
  /**
   * Per-node tag sets — maps a node index to its named tag sets, where each
   * tag set contains a collection of string tags.
   */
  tag_sets: { [key: NodeIDX]: { [key: string]: string[] } };
}
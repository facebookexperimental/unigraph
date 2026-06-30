/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<75ace7d0784fab19bf0648f8b38f4fc0>>
 */


import type { NodeIDX } from './NodeIDX.ts';

/** Serializable per-node metadata: numeric metrics, categorical labels, and string properties. */
export interface ArrayGraphSerializableNodeMetadata {
  /**
   * Named metrics — each entry maps a metric name to a `Vec<f32>` with one
   * value per node (indexed by [`NodeIDX`]).
   */
  metrics: { [key: string]: number[] };
  /**
   * Per-label-name index — maps a label name to the set of nodes that have it,
   * and for each node the set of values for that label.
   */
  labels: { [key: string]: { [key: NodeIDX]: string[] } };
  /**
   * Per-property-name index — maps a property name to the set of nodes that have it,
   * and for each node the single value for that property.
   */
  properties: { [key: string]: { [key: NodeIDX]: string } };
}
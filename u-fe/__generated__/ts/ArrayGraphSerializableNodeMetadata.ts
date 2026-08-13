/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<6f85af625e08016ad14f801403567b20>>
 */


import type { NodeIDX } from './NodeIDX.ts';

/** Serializable per-node metadata: numeric metrics, categorical labels, and string properties. */
export interface ArrayGraphSerializableNodeMetadata {
  /**
   * Named metrics — each entry maps a metric name to a `Vec<f64>` with one
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
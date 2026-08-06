/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0c29658f6548269e2bbe4ad30b9efec3>>
 */


import type { NodeHistory } from './NodeHistory.ts';

export interface GetHistoryOutput {
  /**
   * Metric names in the order every sample chunk's value slots follow the
   * two header slots. Also fixes the chunk stride at
   * `2 + metrics.len()`.
   */
  metrics: string[];
  /**
   * One entry per requested node, sorted by name. A node with no recorded
   * history still gets an entry, with an empty stream.
   */
  series: NodeHistory[];
}
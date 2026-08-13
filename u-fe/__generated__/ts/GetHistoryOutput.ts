/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<82923ff8edd9413fe4a1846e44e18d19>>
 */


import type { NodeHistory } from './NodeHistory.ts';

export interface GetHistoryOutput {
  /**
   * Metric names in the order every sample chunk's value slots follow the
   * four header slots. Also fixes the chunk stride at `4 + metrics.len()`.
   */
  metrics: string[];
  /**
   * One entry per requested node, sorted by name. A node with no recorded
   * history still gets an entry, with an empty stream.
   */
  series: NodeHistory[];
}
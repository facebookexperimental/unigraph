/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<19136d3b92a584ab69ada446a3ca1257>>
 */


import type { HistoryFrame } from './HistoryFrame.ts';
import type { NodeHistory } from './NodeHistory.ts';

export interface GetHistoryOutput {
  /** Metric names in the order every `HistorySample::values` is aligned to. */
  metrics: string[];
  /**
   * Every frame referenced by `series`, deduplicated across nodes and
   * sorted by `(timestamp, graph_id)`.
   */
  frames: HistoryFrame[];
  /**
   * One entry per requested node, sorted by name. A node with no recorded
   * history still gets an entry, with no samples.
   */
  series: NodeHistory[];
}
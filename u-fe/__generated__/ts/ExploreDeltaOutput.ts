/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<355e7ee61d48289ddff6509dea7ca0a7>>
 */


import type { ExploreDeltaArrow } from './ExploreDeltaArrow.ts';

export interface ExploreDeltaOutput {
  /**
   * The node being explored, with its own metrics. None when showing entry
   * points or all nodes.
   */
  node?: ExploreDeltaArrow | undefined;
  /** Arrows to children (or to entry points / all nodes when node is None). */
  arrows: ExploreDeltaArrow[];
  /** Metric names present in either graph. */
  metric_names: string[];
  /** Tier names if tiered traversal is configured. */
  tier_names: string[];
  /** Total number of arrows before offset/limit. */
  total_arrows_count: number;
  /** Unchanged nodes filtered out by `changed_nodes_only`. Always 0 otherwise. */
  hidden_unchanged_count: number;
  /**
   * Human-readable ASCII table of the results. Only populated when
   * `include_ascii` is set to true in the request.
   */
  ascii?: string | undefined;
}
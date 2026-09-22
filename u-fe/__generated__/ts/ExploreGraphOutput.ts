/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<d0cebbf255731c75333716a07aba45be>>
 */


import type { ExploreGraphArrow } from './ExploreGraphArrow.ts';
import type { PerfStats } from './PerfStats.ts';

export interface ExploreGraphOutput {
  /** The node being explored, with its own metrics. None when showing entry points. */
  node?: ExploreGraphArrow | undefined;
  /** Arrows to children (or to entry points when node is None). */
  arrows: ExploreGraphArrow[];
  /** Available metric names in this graph. */
  metric_names: string[];
  /** Tier names if tiered traversal is configured. */
  tier_names: string[];
  /** Total number of arrows before offset/limit. */
  total_arrows_count: number;
  /**
   * Human-readable ASCII table of the results. Only populated when
   * `include_ascii` is set to true in the request.
   */
  ascii?: string | undefined;
  /**
   * Where this request spent its time. `None` from a server predating the
   * field — absent rather than zeroed, so it is never mistaken for a
   * measurement.
   */
  perf_stats?: PerfStats | undefined;
}
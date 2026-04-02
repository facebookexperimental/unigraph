/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AboutGraphMetricInfo } from './AboutGraphMetricInfo.ts';
import type { ArrayGraphStats } from './ArrayGraphStats.ts';

export interface AboutGraphOutput {
  /** Graph description from settings, if available. */
  description?: string | undefined;
  /** Graph statistics (node/edge counts by kind, tier names, etc). */
  stats: ArrayGraphStats;
  /** Available numeric metrics with optional descriptions. */
  metrics: AboutGraphMetricInfo[];
  /**
   * Human-readable markdown summary of the graph.
   * Optimized for LLM consumption — use this field to understand the graph
   * before exploring it with ExploreGraph.
   */
  text: string;
}
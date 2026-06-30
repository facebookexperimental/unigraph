/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<1d27229e03f1e16732be95c0beb30aaf>>
 */


import type { AboutGraphMetricInfo } from './AboutGraphMetricInfo.ts';
import type { ArrayGraphStats } from './ArrayGraphStats.ts';
import type { GraphSettings } from './GraphSettings.ts';

export interface AboutGraphOutput {
  /** Graph description from settings, if available. */
  description?: string | undefined;
  /** Graph statistics (node/edge counts by kind, tier names, etc). */
  stats: ArrayGraphStats;
  /** Per-metric info: description + list of derived views. */
  metrics: AboutGraphMetricInfo[];
  /** All available metric views (flat list). */
  metric_views: string[];
  /** Graph-level settings (description, UI config), if present. */
  graph_settings?: GraphSettings | undefined;
  /** Graph-level key-value properties. */
  properties: { [key: string]: string };
  /**
   * Human-readable markdown summary of the graph.
   * Optimized for LLM consumption — use this field to understand the graph
   * before exploring it with ExploreGraph.
   */
  text: string;
}
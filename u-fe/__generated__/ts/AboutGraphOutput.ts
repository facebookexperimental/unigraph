/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<c39554c24258333e5d15d7b2f3695ebf>>
 */


import type { AboutGraphMetricInfo } from './AboutGraphMetricInfo.ts';
import type { ArrayGraphStats } from './ArrayGraphStats.ts';
import type { GraphID } from './GraphID.ts';
import type { GraphSettings } from './GraphSettings.ts';
import type { TimelineID } from './TimelineID.ts';

export interface AboutGraphOutput {
  /** The timeline the graph the handle resolved to belongs to. */
  timeline_id: TimelineID;
  /**
   * The concrete snapshot the handle resolved to, within `timeline_id`.
   * 
   * Only a `{timeline}~{id}` handle names this directly. A bare timeline
   * means "latest" and a GQC key resolves through its embedded reference —
   * both move as frames are ingested, so everything else in this response is
   * only reproducible when read together with this id.
   */
  graph_id: GraphID;
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
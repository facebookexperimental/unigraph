/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { TraversalConfig } from './TraversalConfig.ts';

/**
 * Configuration for querying a graph — roots to start from, traversal rules,
 * and which graph to query.
 */
export interface GraphQueryConfig {
  roots: string[];
  traversal_config?: TraversalConfig | undefined;
  /**
   * Graph target: timeline ID (`"my_timeline"`) for latest, or
   * `"my_timeline~123"` for a specific snapshot.
   * Uses the same format as `GraphKeyOrTimelineID`.
   */
  handle?: string | undefined;
}
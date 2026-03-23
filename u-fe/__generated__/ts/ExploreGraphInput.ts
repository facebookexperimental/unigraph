/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { GraphQueryConfigKey } from './GraphQueryConfigKey.ts';
import type { GraphStructure } from './GraphStructure.ts';
import type { NodeMetric } from './NodeMetric.ts';
import type { SortOrder } from './SortOrder.ts';

export interface ExploreGraphInput {
  /** Inline graph query config. Either this or `graph_query_config_key` must be set. */
  graph_query_config?: GraphQueryConfig | undefined;
  /** Key referencing a stored graph query config. Resolved server-side. */
  graph_query_config_key?: GraphQueryConfigKey | undefined;
  /** Node to explore. None = show graph entry points. */
  node?: string | undefined;
  /** Which edge structure to follow. */
  graph_structure: GraphStructure;
  /** Which metrics to compute for each arrow. */
  metrics: NodeMetric[];
  /** Metric to sort arrows by. Computed for all children (even beyond limit). */
  sort_by?: NodeMetric | undefined;
  /** Sort order. Defaults to Desc. */
  sort_order?: SortOrder | undefined;
  /** Skip first N results (for pagination). */
  offset?: number | undefined;
  /** Maximum number of arrows to return. Defaults to 50. */
  limit?: number | undefined;
  /**
   * When true, populate the `ascii` field in the response with a human-readable
   * ASCII table of the results (optimized for agent / LLM consumption).
   */
  include_ascii?: boolean | undefined;
}
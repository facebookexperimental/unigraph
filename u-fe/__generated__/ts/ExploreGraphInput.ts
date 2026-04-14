/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ExploreGraphTarget } from './ExploreGraphTarget.ts';
import type { GraphHandle } from './GraphHandle.ts';
import type { GraphStructure } from './GraphStructure.ts';
import type { MetricView } from './MetricView.ts';
import type { SortOrder } from './SortOrder.ts';
import type { TraversalOverride } from './TraversalOverride.ts';

export interface ExploreGraphInput {
  /**
   * Graph handle: timeline ID (`"my_timeline"`), graph key
   * (`"my_timeline~123"`), or GQC key (`"gqc_abc123"`).
   */
  handle: GraphHandle;
  /**
   * Optional roots override. When set, overrides the handle's roots
   * (GQC roots or graph entry points).
   */
  roots?: string[] | undefined;
  /**
   * Optional traversal config override. When set, overrides the handle's
   * traversal config (GQC traversal or graph.traversal_config).
   */
  traversal?: TraversalOverride | undefined;
  /** What to explore: entry points, a specific node, or all nodes. */
  target: ExploreGraphTarget;
  /** Which edge structure to follow. */
  graph_structure: GraphStructure;
  /**
   * Which metrics to compute for each arrow.
   * - `None` (default): return all available metric views.
   * - `Some([])`: return no metrics.
   * - `Some([...])`: return exactly the listed metrics.
   */
  metrics?: MetricView[] | undefined;
  /** Metric to sort arrows by. Computed for all children (even beyond limit). */
  sort_by?: MetricView | undefined;
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
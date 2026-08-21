/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<b817d8c84c9c3a47849ed5d146838b8f>>
 */


import type { ExploreGraphTarget } from './ExploreGraphTarget.ts';
import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { GraphStructure } from './GraphStructure.ts';
import type { MetricView } from './MetricView.ts';
import type { SortOrder } from './SortOrder.ts';

export interface ExploreGraphInput {
  /** The query config: which graph, optional roots, optional traversal. */
  query: GraphQueryConfig;
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
   * When true, also return arrows the traversal did not follow, flagged via
   * `excluded` / `unreachable`. Defaults to false, which drops them entirely.
   * 
   * Only meaningful for the `Node` target — the other targets enumerate
   * nodes, not edges, and already return reachable nodes only.
   */
  include_excluded?: boolean | undefined;
  /**
   * When true, populate the `ascii` field in the response with a human-readable
   * ASCII table of the results (optimized for agent / LLM consumption).
   */
  include_ascii?: boolean | undefined;
}
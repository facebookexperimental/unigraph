/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<701d7756df598b8ecaa99b3bf074d99c>>
 */


import type { ExploreGraphTarget } from './ExploreGraphTarget.ts';
import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { GraphStructure } from './GraphStructure.ts';
import type { SortOrder } from './SortOrder.ts';

export interface ExploreDeltaInput {
  /** The "before" graph. */
  left: GraphQueryConfig;
  /** The "after" graph. Deltas are `right - left`. */
  right: GraphQueryConfig;
  /** What to explore: entry points, a specific node, or all nodes. */
  target: ExploreGraphTarget;
  /** Which edge structure to follow. */
  graph_structure: GraphStructure;
  /**
   * Collapse nodes that are identical on both sides. Rows then report how
   * many unchanged nodes were skipped to reach them.
   */
  changed_nodes_only: boolean;
  /**
   * Which columns to compute.
   * - `None` (default): the right-hand value and `∆` for every visible view.
   * - `Some([])`: no metrics.
   * - `Some([...])`: exactly the listed columns.
   */
  metrics?: string[] | undefined;
  /** Column to sort by. Computed for every row, even beyond the limit. */
  sort_by?: string | undefined;
  /** Sort order. Defaults to Desc. */
  sort_order?: SortOrder | undefined;
  /**
   * Sort `∆` columns by magnitude, so the biggest regressions and the
   * biggest wins both surface at the top. Defaults to true, matching the UI.
   */
  sort_delta_by_magnitude?: boolean | undefined;
  /** Skip first N results (for pagination). */
  offset?: number | undefined;
  /** Maximum number of arrows to return. Defaults to 50. */
  limit?: number | undefined;
  /**
   * When true, populate the `ascii` field with a human-readable table
   * (optimized for agent / LLM consumption).
   */
  include_ascii?: boolean | undefined;
}
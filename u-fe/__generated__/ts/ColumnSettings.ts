/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<63055e55c4ab43726d20a6bc526e1c68>>
 */


import type { GraphTableSort } from './GraphTableSort.ts';

export interface ColumnSettings {
  /**
   * Graph table in UI will be sorted using provided column
   * and order if any
   */
  graph_table_sort?: GraphTableSort | undefined;
  /**
   * Global setting for showing metric values
   * (if tiers are defined)
   * It is shown by default, but can be hidden
   */
  hide_metrics?: boolean | undefined;
  /**
   * Global setting for showing tiered values for metrics
   * (if tiers are defined)
   * It is hidden by default, but can be enabled
   */
  show_tiered_metrics?: boolean | undefined;
  /**
   * Global setting for showing dominated metric values.
   * Defaults to showing because individual values default
   * to only showing when in Dominator mode.
   */
  hide_dominated_tiered_metrics?: boolean | undefined;
  /**
   * Global setting for showing columns related to
   * node counts, like transitive counts or parents counts.
   */
  show_counts?: boolean | undefined;
  /** Show a column that displays the tier each node */
  show_tier_column?: boolean | undefined;
}
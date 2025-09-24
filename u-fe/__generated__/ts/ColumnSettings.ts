/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphTableSort } from './GraphTableSort.ts';
import type { IndividualDominatedOptionEnabled } from './IndividualDominatedOptionEnabled.ts';
import type { IndividualOptionEnabled } from './IndividualOptionEnabled.ts';
import type { MetricSettings } from './MetricSettings.ts';

export interface ColumnSettings {
  /**
   * Graph table in UI will be sorted using provided column
   * and order if any
   */
  graph_table_sort?: GraphTableSort | undefined;
  show_parents_count?: IndividualOptionEnabled | undefined;
  show_transitive_count?: IndividualOptionEnabled | undefined;
  show_conjoint_count?: IndividualOptionEnabled | undefined;
  show_dominated_count?: IndividualDominatedOptionEnabled | undefined;
  /**
   * Global setting for showing metric values
   * (if tiers are defined)
   * It is shown by default, but can be hidden
   */
  hide_metrics?: boolean | undefined;
  /**
   * Global setting for showing tiered values for metrics
   * (if tiers are defined)
   * It is hidden by default, but can be endabled
   */
  show_tiered_metrics?: boolean | undefined;
  /** Global setting for showing conjoint values for tiered metrics */
  show_conjoint_tiered_metrics?: boolean | undefined;
  /**
   * Global setting for showing columns related to
   * node counts, like transitive counts, parents counts,
   * or conjoint cost for node counts.
   * Individual columns will be enabled/disabled based on
   * their individual settings.
   */
  show_counts?: boolean | undefined;
  /**
   * Global setting for showing dominated cost values.
   * Individual columns will be enabled/disabled based on
   * their individual settings.
   */
  show_dominated?: boolean | undefined;
  /** Show a column that displays the tier each node */
  show_tier_column?: boolean | undefined;
  metric_settings?: { [key: string]: MetricSettings } | undefined;
}
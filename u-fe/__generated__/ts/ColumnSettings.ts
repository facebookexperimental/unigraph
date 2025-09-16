/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphTableSort } from './GraphTableSort.ts';
import type { IndividualOptionEnabled } from './IndividualOptionEnabled.ts';
import type { MetricSettings } from './MetricSettings.ts';

export interface ColumnSettings {
  /**
   * Graph table in UI will be sorted using provided column
   * and order if any
   */
  graph_table_sort?: GraphTableSort | undefined;
  show_parents_count?: boolean | undefined;
  show_transitive_count?: IndividualOptionEnabled | undefined;
  show_conjoint_count?: IndividualOptionEnabled | undefined;
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
  show_tiered?: boolean | undefined;
  /**
   * Global setting for showing transitive values.
   * Individual columns will be enabled/disabled based on
   * their individual settings.
   */
  show_transitive?: boolean | undefined;
  /**
   * Global setting for showing conjoint cost values.
   * Individual columns will be enabled/disabled based on
   * their individual settings.
   */
  show_conjoint?: boolean | undefined;
  /** Show a column that displays the tier each node */
  show_tier_column?: boolean | undefined;
  metric_settings?: { [key: string]: MetricSettings } | undefined;
}
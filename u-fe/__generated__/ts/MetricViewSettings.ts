/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { MetricFormat } from './MetricFormat.ts';
import type { MetricViewVisibility } from './MetricViewVisibility.ts';

/**
 * Per-view settings in the flat metric view map.
 * 
 * Keys are `MetricView.to_string()` values (e.g. `"size"`, `"size~transitive"`,
 * `"node-count~dominated"`).
 */
export interface MetricViewSettings {
  /**
   * Controls when this view is shown.
   * `None` = use default for this view type:
   *   - Non-dominated views default to `Enabled`
   *   - Dominated views default to `EnabledInDominatorMode`
   */
  visibility?: MetricViewVisibility | undefined;
  /**
   * Display format. Derived views (transitive, dominated, tiered, conjoint)
   * inherit the format from their base metric key (e.g., `"size"`) if not set here.
   */
  format?: MetricFormat | undefined;
  /** Description. Typically only set on base metric keys. */
  description?: string | undefined;
}
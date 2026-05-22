/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { MetricViewVisibility } from './MetricViewVisibility.ts';

/**
 * Per-view settings in the flat metric view map.
 * 
 * Keys are `MetricView.to_string()` values (e.g. `"size"`, `"size~transitive"`,
 * `"node-count~dominated"`).
 */
export interface MetricViewSettings {
  /**
   * Controls when this view is shown in the UI.
   * `None` = use default for this view type:
   *   - Non-dominated views default to `Enabled`
   *   - Dominated views default to `EnabledInDominatorMode`
   */
  visibility?: MetricViewVisibility | undefined;
}

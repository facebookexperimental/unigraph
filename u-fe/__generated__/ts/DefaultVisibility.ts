/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { MetricViewVisibility } from './MetricViewVisibility.ts';

/**
 * Global defaults for view type visibility.
 * 
 * Resolution: per-view override in `metrics_visibility` →
 * per-type field → `all` → hardcoded (dominated → `EnabledInDominatorMode`,
 * everything else → `Enabled`).
 * 
 * ```text
 * // "Hide everything by default, only show what's explicitly enabled"
 * DefaultVisibility { all: Some(Hidden), .. }
 * 
 * // "Hide tiered, show everything else normally"
 * DefaultVisibility { tiered: Some(Hidden), tiered_dominated: Some(Hidden), .. }
 * 
 * // "Hide everything except tiered"
 * DefaultVisibility { all: Some(Hidden), tiered: Some(Enabled), .. }
 * ```
 */
export interface DefaultVisibility {
  /**
   * Catch-all default. Lowest precedence — overridden by any
   * per-type field below.
   */
  all?: MetricViewVisibility | undefined;
  self_view?: MetricViewVisibility | undefined;
  transitive?: MetricViewVisibility | undefined;
  dominated?: MetricViewVisibility | undefined;
  tiered?: MetricViewVisibility | undefined;
  tiered_dominated?: MetricViewVisibility | undefined;
}
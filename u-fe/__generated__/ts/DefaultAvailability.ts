/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Availability } from './Availability.ts';

/**
 * Global defaults for which view types are available per metric.
 * 
 * Resolution: per-metric field → this default → hardcoded `Available`.
 */
export interface DefaultAvailability {
  self_view?: Availability | undefined;
  transitive?: Availability | undefined;
  dominated?: Availability | undefined;
  /**
   * Set to `Unavailable` to suppress tier columns for all metrics
   * unless individually overridden in `MetricConfig`.
   */
  tiered?: Availability | undefined;
  tiered_dominated?: Availability | undefined;
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<97d53f82653b5fb488d6ae3feee4fecf>>
 */


import type { Availability } from './Availability.ts';
import type { DefaultAvailability } from './DefaultAvailability.ts';
import type { DefaultVisibility } from './DefaultVisibility.ts';
import type { MetricConfig } from './MetricConfig.ts';

/**
 * Graph-builder-authored metric configuration.
 * 
 * Controls both which metric views exist (availability) and which
 * are shown by default (visibility). Per-view visibility overrides
 * live in `GraphSettings.metrics_visibility`.
 * 
 * # Resolution chains
 * 
 * **Availability** (does this view exist?):
 *   1. `metrics["size"].tiered` (per-metric)
 *   2. `default_availability.tiered` (global default)
 *   3. hardcoded `Available`
 * 
 * **Visibility** (is this view shown by default?):
 *   1. `GraphSettings.metrics_visibility["size#eager"]` (per-view override)
 *   2. `default_visibility.tiered` (global default)
 *   3. hardcoded: dominated → `EnabledInDominatorMode`, else → `Enabled`
 * 
 * # Example
 * 
 * ```text
 * MetricsConfig {
 *     default_availability: DefaultAvailability {
 *         tiered: Some(Unavailable),  // no tier columns except overrides
 *     },
 *     default_visibility: DefaultVisibility {
 *         dominated: Some(Hidden),    // dominated hidden by default
 *     },
 *     metrics: {
 *         "size": MetricConfig {
 *             tiered: Some(Available),  // override: size gets tiers
 *             self_view: Some(Unavailable),
 *         },
 *     },
 * }
 * ```
 */
export interface MetricsConfig {
  default_availability?: DefaultAvailability | undefined;
  default_visibility?: DefaultVisibility | undefined;
  /** Per-metric configuration keyed by metric name (e.g. `"size"`). */
  metrics?: { [key: string]: MetricConfig } | undefined;
  parents_count?: Availability | undefined;
  count_transitive?: Availability | undefined;
  count_dominated?: Availability | undefined;
}
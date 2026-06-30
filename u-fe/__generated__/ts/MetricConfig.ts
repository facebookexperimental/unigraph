/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<e9d23ac1973ffbbd210dfec193c91a80>>
 */


import type { Availability } from './Availability.ts';
import type { MetricFormat } from './MetricFormat.ts';

/**
 * Per-metric configuration: which derived view types are available,
 * plus format and description shared by all views of this metric.
 * 
 * Each metric in a graph (e.g. `"size"`, `"build_time"`) can produce
 * up to 5 kinds of views:
 * 
 * - **self_view** — the raw per-node value (e.g. this file is 42 KB)
 * - **transitive** — DFS sum over forward edges (total reachable cost)
 * - **dominated** — DFS sum over the dominator tree (uniquely owned cost)
 * - **tiered** — transitive sum broken down by loading tier (e.g. eager/lazy)
 * - **tiered_dominated** — dominated sum broken down by tier
 * 
 * All fields are optional. `None` means "inherit from the global default
 * in `MetricsConfig`." This lets you set a project-wide policy and only
 * override specific metrics.
 * 
 * `format` and `description` are shared across all views of this metric
 * (the format for `size~transitive` is the same as `size` — both are bytes).
 * 
 * ```text
 * // "size" metric: only show tiered views, hide everything else
 * MetricConfig {
 *     self_view:        Some(Unavailable),
 *     transitive:       Some(Unavailable),
 *     dominated:        None,              // inherits global default
 *     tiered:           Some(Available),
 *     tiered_dominated: Some(Available),
 *     format:           Some(Size { .. }),
 *     description:      Some("File size in bytes"),
 * }
 * 
 * // "impact_count": precomputed value, derived views make no sense
 * MetricConfig {
 *     self_view:        Some(Available),
 *     transitive:       Some(Unavailable),
 *     dominated:        Some(Unavailable),
 *     tiered:           Some(Unavailable),
 *     tiered_dominated: Some(Unavailable),
 *     ..
 * }
 * ```
 */
export interface MetricConfig {
  /** The raw per-node value. Hide when only tiered views matter. */
  self_view?: Availability | undefined;
  /** Transitive sum over forward edges. */
  transitive?: Availability | undefined;
  /** Transitive sum over the dominator tree (uniquely owned cost). */
  dominated?: Availability | undefined;
  /** Transitive sum broken down by loading tier (one column per tier). */
  tiered?: Availability | undefined;
  /** Dominated sum broken down by loading tier. */
  tiered_dominated?: Availability | undefined;
  /** Display format inherited by all views of this metric. */
  format?: MetricFormat | undefined;
  /** Human-readable description of what this metric measures. */
  description?: string | undefined;
}
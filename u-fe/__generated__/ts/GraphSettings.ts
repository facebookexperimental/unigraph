/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphUISettings } from './ArrayGraphUISettings.ts';
import type { MetricViewVisibility } from './MetricViewVisibility.ts';
import type { MetricsConfig } from './MetricsConfig.ts';

export interface GraphSettings {
  description?: string | undefined;
  /**
   * Which metric views exist at all (availability layer).
   * See `MetricsConfig` for details and examples.
   */
  metrics_config?: MetricsConfig | undefined;
  /**
   * Per-view UI visibility overrides keyed by `MetricView.to_string()`.
   * Controls which available views are shown/hidden by default.
   * Views not listed here use their type-specific default
   * (non-dominated → shown, dominated → shown in dominator mode).
   */
  metrics_visibility?: { [key: string]: MetricViewVisibility } | undefined;
  /** UI presentation settings (columns, sort, sidebar, entry points). */
  ui_settings?: ArrayGraphUISettings | undefined;
}
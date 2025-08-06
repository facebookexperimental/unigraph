/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphUISettings } from './ArrayGraphUISettings.ts';
import type { MetricSettings } from './MetricSettings.ts';

export interface GraphSettings {
  metric_settings?: { [key: string]: MetricSettings } | undefined;
  ui_settings?: ArrayGraphUISettings | undefined;
}
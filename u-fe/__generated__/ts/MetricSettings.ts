/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { IndividualOptionEnabled } from './IndividualOptionEnabled.ts';
import type { MetricFormat } from './MetricFormat.ts';

export interface MetricSettings {
  description?: string | undefined;
  format?: MetricFormat | undefined;
  /** Hide table column that displays the metric itself. */
  column_hide_self?: boolean | undefined;
  /** Column that displays transitive value for the metric. */
  column_show_transitive?: IndividualOptionEnabled | undefined;
  column_show_tiered?: { [key: string]: IndividualOptionEnabled } | undefined;
  show_conjoint_self?: IndividualOptionEnabled | undefined;
  show_conjoint_tiered?: { [key: string]: IndividualOptionEnabled } | undefined;
}
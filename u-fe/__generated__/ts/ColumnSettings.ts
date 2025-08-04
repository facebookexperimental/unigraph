import type { IndividualOptionEnabled } from './IndividualOptionEnabled.ts';

export interface ColumnSettings {
  show_parents_count?: boolean | undefined;
  show_transitive_count?: IndividualOptionEnabled | undefined;
  show_conjoint_count?: IndividualOptionEnabled | undefined;
  /** Global setting for showing metric values (if tiers are defined) It is shown by default, but can be hidden */
  hide_metrics?: boolean | undefined;
  /** Global setting for showing tiered values for metrics (if tiers are defined) It is hidden by default, but can be endabled */
  show_tiered?: boolean | undefined;
  /** Global setting for showing transitive values. Individual columns will be enabled/disabled based on their individual settings. */
  show_transitive?: boolean | undefined;
  /** Global setting for showing conjoint cost values. Individual columns will be enabled/disabled based on their individual settings. */
  show_conjoint?: boolean | undefined;
}
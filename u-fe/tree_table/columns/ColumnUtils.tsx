// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GraphStructure } from "../../__generated__/ts/GraphStructure";
import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import type { MetricsConfig } from "../../__generated__/ts/MetricsConfig";
import type { MetricViewVisibility } from "../../__generated__/ts/MetricViewVisibility";

// ── MetricView key builders ────────────────────────────────────
// Must match Rust `MetricView::Display` output exactly.

const SEP = "~";
const TIER_SEP = "#";
const SIDE_SEP = "@";

export const MV = {
  metric: (name: string) => name,
  transitive: (name: string) => `${name}${SEP}transitive`,
  dominated: (name: string) => `${name}${SEP}dominated`,
  tiered: (name: string, tier: string) => `${name}${TIER_SEP}${tier}`,
  tieredDominated: (name: string, tier: string) =>
    `${name}${TIER_SEP}${tier}${SEP}dominated`,
  parentsCount: "parents-count",
  countTransitive: `node-count${SEP}transitive`,
  countDominated: `node-count${SEP}dominated`,
  tierIndex: "tier",

  left: (key: string) => `${key}${SIDE_SEP}left`,
  delta: (key: string) => `${key}${SIDE_SEP}delta`,
};

// ── Visibility helpers ─────────────────────────────────────────

/**
 * For non-dominated views: undefined defaults to enabled (shown).
 */
export function isViewVisible(
  visibility: MetricViewVisibility | undefined,
): boolean {
  if (visibility == null) return true;
  return visibility === "Enabled";
}

/**
 * For dominated views: undefined defaults to showing only in dominator mode.
 */
export function isEnabledForGraphStructure(
  graphStructure: GraphStructure = "Forward",
  visibility: MetricViewVisibility | undefined,
): boolean {
  if (visibility == null) return graphStructure === "Dominator";
  if (visibility === "Enabled") return true;
  if (visibility === "EnabledInDominatorMode")
    return graphStructure === "Dominator";
  return false;
}

// ── Visibility value constructors ──────────────────────────────

export const ENABLED: MetricViewVisibility = "Enabled";
export const ENABLED_IN_DOMINATOR: MetricViewVisibility =
  "EnabledInDominatorMode";
export const HIDDEN: MetricViewVisibility = "Hidden";

export function isVisibleForStructure(
  vis: MetricViewVisibility,
  graphStructure: GraphStructure,
): boolean {
  if (vis === "Enabled") return true;
  if (vis === "EnabledInDominatorMode") return graphStructure === "Dominator";
  return false;
}

// ── MetricsConfig helpers ─────────────────────────────────────

export function metricFormatFromConfig(
  metricsConfig: MetricsConfig | undefined,
  metricName: string,
): MetricFormat | undefined {
  return metricsConfig?.metrics?.[metricName]?.format;
}

export function isMetricAvailable(
  metricsConfig: MetricsConfig | undefined,
  metricName: string,
  viewType:
    | "self_view"
    | "transitive"
    | "dominated"
    | "tiered"
    | "tiered_dominated",
): boolean {
  if (metricsConfig == null) return true;
  const mc = metricsConfig.metrics?.[metricName];
  const perMetric = mc?.[viewType];
  if (perMetric != null) return perMetric === "Available";
  const defaultKey = `default_${viewType}` as keyof MetricsConfig;
  const globalDefault = metricsConfig[defaultKey] as string | undefined;
  if (globalDefault != null) return globalDefault === "Available";
  return true;
}

export function isStructuralAvailable(
  metricsConfig: MetricsConfig | undefined,
  key: "parents_count" | "count_transitive" | "count_dominated",
): boolean {
  if (metricsConfig == null) return true;
  return (metricsConfig[key] ?? "Available") === "Available";
}

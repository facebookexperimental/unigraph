// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GraphSettings } from "../__generated__/ts/GraphSettings";
import type { GraphStructure } from "../__generated__/ts/GraphStructure";
import type { MetricFormat } from "../__generated__/ts/MetricFormat";
import type { MetricsConfig } from "../__generated__/ts/MetricsConfig";
import type { MetricViewVisibility } from "../__generated__/ts/MetricViewVisibility";

export type ViewType =
  | "self_view"
  | "transitive"
  | "dominated"
  | "tiered"
  | "tiered_dominated";

const DOMINATED_VIEW_TYPES: ReadonlySet<ViewType> = new Set([
  "dominated",
  "tiered_dominated",
]);

export class MetricsConfigResolver {
  private metricsConfig: MetricsConfig | undefined;
  private overrides: { [key: string]: MetricViewVisibility } | undefined;

  constructor(graphSettings: GraphSettings) {
    this.metricsConfig = graphSettings.metrics_config;
    this.overrides = graphSettings.metrics_visibility;
  }

  // ── Availability ──────────────────────────────────────────

  isAvailable(metricName: string, viewType: ViewType): boolean {
    return resolve(
      this.metricsConfig?.metrics?.[metricName]?.[viewType],
      this.metricsConfig?.default_availability?.[viewType],
      "Available",
    );
  }

  isStructuralAvailable(
    key: "parents_count" | "count_transitive" | "count_dominated",
  ): boolean {
    return (this.metricsConfig?.[key] ?? "Available") === "Available";
  }

  hasTiersAvailable(metricName: string): boolean {
    return (
      this.isAvailable(metricName, "tiered") ||
      this.isAvailable(metricName, "tiered_dominated")
    );
  }

  // ── Visibility ────────────────────────────────────────────

  isVisible(
    viewKey: string,
    viewType: ViewType,
    graphStructure: GraphStructure,
  ): boolean {
    const vis = this.resolvedVisibility(viewKey, viewType);
    return resolveVisibility(vis, graphStructure);
  }

  resolvedVisibility(
    viewKey: string,
    viewType: ViewType,
  ): MetricViewVisibility {
    const explicit = this.overrides?.[viewKey];
    if (explicit != null) return explicit;
    const perType = this.metricsConfig?.default_visibility?.[viewType];
    if (perType != null) return perType;
    const all = this.metricsConfig?.default_visibility?.all;
    if (all != null) return all;
    return hardcodedDefault(viewType);
  }

  // ── Format / Description ──────────────────────────────────

  format(metricName: string): MetricFormat | undefined {
    return this.metricsConfig?.metrics?.[metricName]?.format;
  }

  // ── Setters (immutable updates to GraphSettings) ──────────

  setVisibility(
    graphSettings: GraphSettings,
    viewKey: string,
    visibility: MetricViewVisibility,
  ): GraphSettings {
    return {
      ...graphSettings,
      metrics_visibility: {
        ...graphSettings.metrics_visibility,
        [viewKey]: visibility,
      },
    };
  }
}

// ── Helpers ──────────────────────────────────────────────────

function resolve(
  perMetric: string | undefined,
  globalDefault: string | undefined,
  hardcoded: string,
): boolean {
  return (perMetric ?? globalDefault ?? hardcoded) === "Available";
}

function resolveVisibility(
  vis: MetricViewVisibility,
  graphStructure: GraphStructure,
): boolean {
  if (vis === "Enabled") return true;
  if (vis === "EnabledInDominatorMode") return graphStructure === "Dominator";
  return false;
}

function hardcodedDefault(viewType: ViewType): MetricViewVisibility {
  return DOMINATED_VIEW_TYPES.has(viewType)
    ? "EnabledInDominatorMode"
    : "Enabled";
}

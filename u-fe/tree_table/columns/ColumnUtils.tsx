// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GraphStructure } from "../../__generated__/ts/GraphStructure";
import type { MetricViewVisibility } from "../../__generated__/ts/MetricViewVisibility";

// ── MetricView key builders ────────────────────────────────────
// Must match Rust `MetricView::Display` output exactly.

const SEP = "~";
const SIDE_SEP = "@";

export const MV = {
  metric: (name: string) => name,
  transitive: (name: string) => `${name}${SEP}transitive`,
  dominated: (name: string) => `${name}${SEP}dominated`,
  tiered: (name: string, tier: string) => `${name}${SEP}${tier}`,
  tieredDominated: (name: string, tier: string) =>
    `${name}${SEP}dominated${SEP}${tier}`,
  conjointTiered: (name: string, tier: string) =>
    `${name}${SEP}conjoint${SEP}${tier}`,
  parentsCount: "parents-count",
  countTransitive: `node-count${SEP}transitive`,
  countDominated: `node-count${SEP}dominated`,
  countConjoint: `node-count${SEP}conjoint`,

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
  if ("Enabled" in visibility) return true;
  return false;
}

/**
 * For dominated views: undefined defaults to showing only in dominator mode.
 */
export function isEnabledForGraphStructure(
  graphStructure: GraphStructure = "Forward",
  visibility: MetricViewVisibility | undefined,
): boolean {
  if (visibility == null) {
    return graphStructure === "Dominator";
  }
  if ("Enabled" in visibility) return true;
  if ("EnabledInDominatorMode" in visibility)
    return graphStructure === "Dominator";
  return false;
}

// ── Visibility value constructors ──────────────────────────────

export const ENABLED: MetricViewVisibility = { Enabled: {} };
export const ENABLED_IN_DOMINATOR: MetricViewVisibility = {
  EnabledInDominatorMode: {},
};
export const HIDDEN: MetricViewVisibility = { Hidden: {} };

/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { DynamicEdgeInfo } from './DynamicEdgeInfo.ts';

export interface ExploreGraphArrow {
  /** Node name. */
  name: string;
  /**
   * Flat metrics map. Keys follow naming conventions:
   * - "{metric}" — self value
   * - "{metric}_transitive" — transitive sum
   * - "{metric}_dominated" — dominated sum
   * - "{metric}_{tier}" — tiered transitive (if tiers configured)
   * - "parents_count" — number of configured parents
   * - "children_count" — number of children in current graph structure
   */
  metrics: { [key: string]: number };
  /** Edge tag (e.g. "lazy"), if this is a tagged edge. */
  tag?: string | undefined;
  /** Dynamic edge info, if this is a dynamic edge. */
  dynamic?: DynamicEdgeInfo | undefined;
}
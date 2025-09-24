/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ColumnType } from './ColumnType.ts';

export type SortColumn =
  /** Sort by node name (tree column) */
  { "NodeName": {  } } |
  /** Transitive count column */
  { "TransitiveCount": { t: ColumnType } } |
  /** Transitive dominated count column */
  { "DominatedCount": { t: ColumnType } } |
  /** Number of parents for each node */
  { "ParentsCount": { t: ColumnType } } |
  /** Metric column for specified metric */
  { "Metric": { t: ColumnType, name: string } } |
  /**
   * Metric column for specified metric (Right Graph)
   * Transitive metric column for specified metric
   */
  { "TransitiveMetric": { t: ColumnType, name: string } } |
  /** Dominated metric column for specified metric */
  { "DominatedMetric": { t: ColumnType, name: string } } |
  /** Tiered transitive metric column for specified metric */
  { "TieredTransitiveMetric": { t: ColumnType, name: string, tier: string } } |
  /** Conjoint count */
  { "ConjointCount": { t: ColumnType } } |
  /** Conjoint tiered metric */
  { "ConjointTieredMetric": { t: ColumnType, name: string, tier: string } } |
  /** Conjoint tiered metric */
  { "DominatedTieredMetric": { t: ColumnType, name: string, tier: string } };

export type SortColumnVariants = "NodeName" | "TransitiveCount" | "DominatedCount" | "ParentsCount" | "Metric" | "TransitiveMetric" | "DominatedMetric" | "TieredTransitiveMetric" | "ConjointCount" | "ConjointTieredMetric" | "DominatedTieredMetric";
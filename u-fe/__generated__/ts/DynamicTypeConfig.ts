/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<08bd0ba841c60436cc2174c6f91cab71>>
 */


import type { DefaultBranches } from './DefaultBranches.ts';
import type { DynamicEdgeOverride } from './DynamicEdgeOverride.ts';

/**
 * Config for all dynamic edges of a given type (e.g. "ddd", "rc:gk").
 * 
 * Resolution order: edge-specific override (branches → decision) → default_branches.
 */
export interface DynamicTypeConfig {
  /** Branch filter applied to edges that have no matching override. */
  default_branches?: DefaultBranches | undefined;
  /** Per-edge-name overrides, checked before default_branches. */
  overrides?: { [key: string]: DynamicEdgeOverride } | undefined;
}
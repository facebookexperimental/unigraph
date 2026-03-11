/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { DefaultBranches } from './DefaultBranches.ts';
import type { DynamicEdgeOverride } from './DynamicEdgeOverride.ts';

export interface DynamicTypeConfig {
  default_branches?: DefaultBranches | undefined;
  overrides?: { [key: string]: DynamicEdgeOverride } | undefined;
}
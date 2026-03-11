/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Decision } from './Decision.ts';
import type { DefaultBranches } from './DefaultBranches.ts';

export interface DynamicEdgeOverride {
  branches?: DefaultBranches | undefined;
  decision?: Decision | undefined;
}
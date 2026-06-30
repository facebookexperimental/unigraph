/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<80093e3ed75e16bde2d7b1b9ceb74ebc>>
 */


import type { Decision } from './Decision.ts';
import type { DefaultBranches } from './DefaultBranches.ts';

/**
 * Override for a specific dynamic edge name.
 * 
 * `branches` handles branches that are explicitly listed in the filter.
 * `decision` is a fallback for branches NOT listed — e.g. a new branch
 * appears in a newer graph that the TVC didn't know about when it was built.
 * 
 * Example: override has `branches: Include(["A", "B"])` and `decision: include`.
 *   - Branch "A" → listed → included (by filter)
 *   - Branch "B" → listed → included (by filter)
 *   - Branch "X" (new, unknown) → not listed → falls back to `decision` → included
 */
export interface DynamicEdgeOverride {
  /** Per-branch filter. Only applies to branches explicitly listed in it. */
  branches?: DefaultBranches | undefined;
  /** Fallback for branches not listed in `branches`. Covers unknown/new branches. */
  decision?: Decision | undefined;
}
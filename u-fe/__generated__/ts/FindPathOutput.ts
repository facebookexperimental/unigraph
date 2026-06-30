/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f40eb69e2f7eca5e6a3b1fc979fe1fb9>>
 */


import type { PathHop } from './PathHop.ts';

export interface FindPathOutput {
  /**
   * The path from `from` to `to`, including edge info per hop.
   * Empty if no path exists.
   */
  path: PathHop[];
  /** Whether a path was found. */
  found: boolean;
  /** Human-readable summary. Only populated when `include_ascii` is true. */
  ascii?: string | undefined;
}
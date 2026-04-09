/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
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
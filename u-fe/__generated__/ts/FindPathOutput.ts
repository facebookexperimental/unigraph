/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<cfab18a40de01b809c62806d162e49cb>>
 */


import type { PathHop } from './PathHop.ts';
import type { PerfStats } from './PerfStats.ts';

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
  /**
   * Where this request spent its time. `None` from a server predating the
   * field — absent rather than zeroed, so it is never mistaken for a
   * measurement.
   */
  perf_stats?: PerfStats | undefined;
}
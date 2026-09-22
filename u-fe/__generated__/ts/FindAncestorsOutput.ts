/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<2d85850dbaaf4f8853b124da3f61c658>>
 */


import type { PerfStats } from './PerfStats.ts';

export interface FindAncestorsOutput {
  /** Matching ancestor node names (paginated). */
  ancestors: string[];
  /** Total number of matching ancestors (before offset/limit). */
  total_count: number;
  /** Human-readable summary. Only populated when `include_ascii` is true. */
  ascii?: string | undefined;
  /**
   * Where this request spent its time. `None` from a server predating the
   * field — absent rather than zeroed, so it is never mistaken for a
   * measurement.
   */
  perf_stats?: PerfStats | undefined;
}
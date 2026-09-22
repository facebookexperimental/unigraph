/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f5df11c4895baf987ddeb7fd84fd687d>>
 */


import type { PerfStats } from './PerfStats.ts';
import type { SearchNodeMatch } from './SearchNodeMatch.ts';

export interface SearchNodesOutput {
  matches: SearchNodeMatch[];
  /**
   * Where this request spent its time. `None` from a server predating the
   * field — absent rather than zeroed, so it is never mistaken for a
   * measurement.
   */
  perf_stats?: PerfStats | undefined;
}
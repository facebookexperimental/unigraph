/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<2d3abd047108d57e1ca0d1a09fe7e217>>
 */


import type { ArrayGraphSerializablePackageBase64 } from './ArrayGraphSerializablePackageBase64.ts';
import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { PerfStats } from './PerfStats.ts';

export interface GraphQueryOutput {
  package: ArrayGraphSerializablePackageBase64;
  graph_query_config: GraphQueryConfig;
  /**
   * The resolved graph key of the snapshot this query landed on, formatted as
   * `"{timeline}~{graph_id}"` (e.g. `"www-budget~223"`). Unlike
   * `graph_query_config.handle` — which merely echoes the input handle — this
   * always carries the concrete timeline and `graph_id`, even when an
   * anonymous `gqc_…` or bare (latest) handle was sent. Lets clients pin
   * follow-up links to the exact version rendered, and resolve
   * timeline-specific behaviour once the graph is known.
   */
  graph_key: string;
  /**
   * Where this request spent its time. `None` from a server predating the
   * field — absent rather than zeroed, so it is never mistaken for a
   * measurement.
   */
  perf_stats?: PerfStats | undefined;
}
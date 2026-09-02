/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<701330a1aa648c80ec457a5c89730606>>
 */


import type { ArrayGraphSerializablePackageBase64 } from './ArrayGraphSerializablePackageBase64.ts';
import type { GraphQueryConfig } from './GraphQueryConfig.ts';

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
}
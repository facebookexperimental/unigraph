/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { GraphQueryConfigKey } from './GraphQueryConfigKey.ts';

export interface GraphQueryInput {
  /** Inline graph query config. Either this or `graph_query_config_key` must be set. */
  graph_query_config?: GraphQueryConfig | undefined;
  /** Key referencing a stored graph query config. Resolved server-side. */
  graph_query_config_key?: GraphQueryConfigKey | undefined;
}
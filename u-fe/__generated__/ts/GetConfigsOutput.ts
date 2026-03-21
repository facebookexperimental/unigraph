/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { TraversalConfig } from './TraversalConfig.ts';

export interface GetConfigsOutput {
  traversal_configs: TraversalConfig[];
  graph_query_configs: GraphQueryConfig[];
}
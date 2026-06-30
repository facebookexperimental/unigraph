/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<eb502ec68b21f279d7bd0e3b76b1af3c>>
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { TraversalConfig } from './TraversalConfig.ts';

export interface GetConfigsOutput {
  traversal_configs: TraversalConfig[];
  graph_query_configs: GraphQueryConfig[];
}
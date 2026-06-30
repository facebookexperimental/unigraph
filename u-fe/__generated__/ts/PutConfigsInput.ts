/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<c2e4f720b6d93cbb594a15239b746b85>>
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { TraversalConfig } from './TraversalConfig.ts';

export interface PutConfigsInput {
  traversal_configs: TraversalConfig[];
  graph_query_configs: GraphQueryConfig[];
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<69e257e5d7fe233219da0d10d8d6e31c>>
 */


import type { GraphQueryConfigKey } from './GraphQueryConfigKey.ts';
import type { TraversalConfigKey } from './TraversalConfigKey.ts';

export interface PutConfigsOutput {
  traversal_configs: TraversalConfigKey[];
  graph_query_configs: GraphQueryConfigKey[];
}
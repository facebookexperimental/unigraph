/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphQueryConfigKey } from './GraphQueryConfigKey.ts';
import type { TraversalConfigKey } from './TraversalConfigKey.ts';

export interface PutConfigsOutput {
  traversal_configs: TraversalConfigKey[];
  graph_query_configs: GraphQueryConfigKey[];
}
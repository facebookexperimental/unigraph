/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<d64bc6e3e28474e23dedd9fe4cc8bfee>>
 */


import type { GraphQueryConfigKey } from './GraphQueryConfigKey.ts';
import type { TraversalConfigKey } from './TraversalConfigKey.ts';

export interface GetConfigsInput {
  traversal_configs: TraversalConfigKey[];
  graph_query_configs: GraphQueryConfigKey[];
}
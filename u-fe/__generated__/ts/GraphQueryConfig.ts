/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphHandle } from './GraphHandle.ts';
import type { TraversalOverride } from './TraversalOverride.ts';

/**
 * Configuration for querying a graph — which graph to query, where to start,
 * and how to traverse.
 * 
 * - `handle`: identifies the graph (timeline, snapshot, or saved GQC key)
 * - `roots`: optional entry points override. `None` = use defaults,
 *   `Some(empty)` = explicitly empty roots (no entrypoints).
 * - `traversal`: optional traversal override (inline config or stored key)
 */
export interface GraphQueryConfig {
  handle: GraphHandle;
  roots?: string[] | undefined;
  traversal?: TraversalOverride | undefined;
}
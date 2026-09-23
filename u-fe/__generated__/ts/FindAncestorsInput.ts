/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<d003a320cfbb5a605a45e0a539bc9b00>>
 */


import type { GraphHandle } from './GraphHandle.ts';
import type { NodeSelection } from './NodeSelection.ts';

export interface FindAncestorsInput {
  /** Graph handle — timeline ID, graph key, or GQC key. */
  handle: GraphHandle;
  /** The node to find ancestors of. */
  node_name: string;
  /**
   * Which ancestors to keep — the same predicate `SearchNodes` and the
   * `Matching` explore target take. An empty selection keeps every ancestor.
   */
  selection: NodeSelection;
  /**
   * When true, only return ancestors with no parents (graph entrypoints).
   * 
   * Not a [`NodeSelection`] condition: that describes edges a node *has*,
   * and this is a condition on the absence of every incoming edge.
   */
  parentless?: boolean | undefined;
  /** Skip first N matching results (for pagination). Defaults to 0. */
  offset?: number | undefined;
  /** Maximum number of results to return. Defaults to 100. */
  limit?: number | undefined;
  /** When true, include a human-readable ASCII summary in the response. */
  include_ascii?: boolean | undefined;
}
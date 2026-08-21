/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<26f3022b3a8e21ca44678bff63d823ae>>
 */


import type { GraphQueryConfig } from './GraphQueryConfig.ts';
import type { MinCutNamedEdge } from './MinCutNamedEdge.ts';

export interface MinCutInput {
  /** The query config: which graph, optional roots, optional traversal. */
  query: GraphQueryConfig;
  /**
   * Nodes to sever from the graph's entry points — a whole feature, not just
   * one module. One cut is computed for the entire set, which is what makes
   * it minimal: severing a shared parent counts once, not once per sink.
   */
  sinks: string[];
  /**
   * Edges that must never be cut. The result is the smallest cut avoiding all
   * of them — the same size or larger than the unconstrained cut, never
   * smaller. Protecting every path to a sink makes the cut impossible, which
   * surfaces as `blocked_by_protected`.
   */
  protected_edges: MinCutNamedEdge[];
  /** Skip first N cut edges (for pagination). Defaults to 0. */
  offset?: number | undefined;
  /** Maximum number of cut edges to return. Defaults to 100. */
  limit?: number | undefined;
  /**
   * When true, populate the `ascii` field in the response with a
   * human-readable rendering (optimized for agent / LLM consumption).
   */
  include_ascii?: boolean | undefined;
}
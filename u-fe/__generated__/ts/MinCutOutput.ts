/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<4c33bdd2f1e7ce3be4d1fae0916feaf8>>
 */


import type { MinCutNamedEdge } from './MinCutNamedEdge.ts';

export interface MinCutOutput {
  /**
   * The edges to remove, sorted by name (paginated). Removing all of them
   * makes every cuttable sink unreachable from every entry point. Empty when
   * the sinks are already unreachable, or when `blocked_by_protected` is set.
   */
  cut_edges: MinCutNamedEdge[];
  /** Total number of cut edges before offset/limit. */
  total_cut_edges_count: number;
  /**
   * Sinks that are themselves entry points. No edge removal can make these
   * unreachable — you have to delete the module. `cut_edges` covers only the
   * remaining, cuttable sinks.
   */
  uncuttable_sinks: string[];
  /**
   * True when the sinks hang off the entry points *only* through protected
   * edges, so no cut avoiding them exists. `cut_edges` is then empty.
   */
  blocked_by_protected: boolean;
  /**
   * Human-readable rendering of the result. Only populated when
   * `include_ascii` is set to true in the request.
   */
  ascii?: string | undefined;
}
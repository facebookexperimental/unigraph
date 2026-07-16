/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<704259562ce021deede67139699039db>>
 */


import type { MinCutEdge } from './MinCutEdge.ts';

/**
 * Serialization-friendly form of [`MinCut`] for transport across the WASM
 * boundary. Tuples don't survive TypeGen cleanly, so `cut_edges` uses a named
 * [`MinCutEdge`] struct instead of `(NodeIDX, NodeIDX)`.
 */
export interface MinCutResult {
  cut_edges: MinCutEdge[];
  has_uncuttable_sink: boolean;
  blocked_by_protected: boolean;
}
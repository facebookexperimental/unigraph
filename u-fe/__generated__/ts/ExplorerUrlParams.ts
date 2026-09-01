/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<e2ef94de39dee514bb3fb27f6e9545d8>>
 */


import type { TraversalOverride } from './TraversalOverride.ts';

/**
 * Every explorer URL search param, flat and all-optional.
 * 
 * Flat rather than nested so it maps one-to-one onto query-string keys: each
 * field name *is* the param name, which is what lets consumers build and read
 * URLs off the generated type instead of scattered string literals.
 */
export interface ExplorerUrlParams {
  /** Entry-point override for both sides. */
  roots?: string[] | undefined;
  /** Entry-point override for the left ("before") graph only. */
  roots_left?: string[] | undefined;
  /** Entry-point override for the right ("after") graph only. */
  roots_right?: string[] | undefined;
  /** Traversal override for both sides. */
  traversal?: TraversalOverride | undefined;
  /** Traversal override for the left ("before") graph only. */
  traversal_left?: TraversalOverride | undefined;
  /** Traversal override for the right ("after") graph only. */
  traversal_right?: TraversalOverride | undefined;
  /**
   * Metric/column view settings, zstd+base64 encoded. Single instance rather
   * than a per-side pair: the explorer holds one settings object, defaulting
   * to the right graph.
   * 
   * Opaque for the same reason as the deltas below — it is not JSON, so it
   * cannot be hand-written, and decoding it needs the WASM codec.
   */
  graph_settings?: string | undefined;
  /**
   * Opaque `GraphQueryConfig` delta the traversal editor writes, per side.
   * 
   * Kept opaque here on purpose: it is a compact encoding of a UI edit, not
   * something anyone hand-writes. It stays separate from `traversal_*` because
   * a full inline `TraversalConfig` can exceed 100 KB while its delta is a few
   * hundred bytes, so the editor cannot round-trip through `traversal_*`.
   */
  gqc_delta_left?: string | undefined;
  /** See [`gqc_delta_left`](Self::gqc_delta_left). */
  gqc_delta_right?: string | undefined;
}
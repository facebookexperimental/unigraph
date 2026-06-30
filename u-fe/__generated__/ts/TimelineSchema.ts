/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<5b1c92ef44b8106109f8eb5a50f30eb7>>
 */


import type { AdjacentDeltasConfig } from './AdjacentDeltasConfig.ts';
import type { FullOrDeltaConfig } from './FullOrDeltaConfig.ts';

/** Schema that governs how frames in a timeline relate to each other. */
export type TimelineSchema =
  /**
   * Simple schema where deltas can reference any graph as a base.
   * 
   * No ordering constraints, no adjacent-base requirements. Deltas are
   * created explicitly via `store_as_delta_from` and can reference graphs
   * in other timelines. Compaction is not supported.
   */
  { "FullOrDelta": FullOrDeltaConfig } |
  /**
   * Linear history where deltas derive from the immediately preceding graph.
   * 
   * Enforces monotonic `(timestamp, graph_id)` ordering and adjacent delta
   * base references. Supports compaction (replacing Full frames with Deltas).
   * Optimized for high-throughput timelines with iterative range-query fetch.
   */
  { "AdjacentDeltas": AdjacentDeltasConfig };

export type TimelineSchemaVariants = "FullOrDelta" | "AdjacentDeltas";
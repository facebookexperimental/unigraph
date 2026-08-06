/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<6a4042ccae5bbda042c2463fffd2d99e>>
 */


import type { TimelineID } from './TimelineID.ts';

export interface GetHistoryInput {
  timeline_id: TimelineID;
  /**
   * Nodes to read. Must be non-empty — history is far too large to return
   * a whole timeline's worth unfiltered.
   */
  node_names: string[];
  /** Inclusive lower bound on sample timestamp, RFC3339 (e.g. `2026-08-05T16:00:00Z`). */
  timestamp_start?: string | undefined;
  /** Inclusive upper bound on sample timestamp, RFC3339. */
  timestamp_end?: string | undefined;
}
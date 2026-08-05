/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<dfd0ed9aa614d7d906e360ccff4f444f>>
 */


import type { TimelineID } from './TimelineID.ts';

export interface SelectFramesInput {
  timeline_id: TimelineID;
  limit?: number | undefined;
  frame_types?: string[] | undefined;
  order?: string | undefined;
  /** Inclusive lower bound on frame timestamp, RFC3339 (e.g. `2026-08-05T16:00:00Z`). */
  timestamp_start?: string | undefined;
  /** Inclusive upper bound on frame timestamp, RFC3339. */
  timestamp_end?: string | undefined;
}
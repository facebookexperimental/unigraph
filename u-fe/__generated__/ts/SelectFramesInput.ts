/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<8ab8ec7162c7c8dfb499392fe56a9e38>>
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
  /**
   * Populate [`FrameInfo::error`] for `Error` frames. Off by default — each
   * error frame costs a full-data row read plus blob resolution.
   */
  include_error_info?: boolean | undefined;
}
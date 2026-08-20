/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<7b948866e7ad1b6d953db8822a47acaa>>
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
   * Only return frames with these graph_ids. Compiles to a SQL `IN`, so it
   * answers "which of these graphs exist" in one round trip rather than
   * paging a busy timeline looking for them.
   */
  graph_ids?: number[] | undefined;
  /**
   * Populate [`FrameInfo::error`] for `Error` frames. Off by default — each
   * error frame costs a full-data row read plus blob resolution.
   */
  include_error_info?: boolean | undefined;
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { TimelineID } from './TimelineID.ts';

export interface SelectFramesInput {
  timeline_id: TimelineID;
  limit?: number | undefined;
  frame_types?: string[] | undefined;
  order?: string | undefined;
}
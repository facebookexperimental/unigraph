/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<74998f8ee29892535cafe10a10ce814e>>
 */


import type { TimelineID } from './TimelineID.ts';

export interface SelectFramesInput {
  timeline_id: TimelineID;
  limit?: number | undefined;
  frame_types?: string[] | undefined;
  order?: string | undefined;
}
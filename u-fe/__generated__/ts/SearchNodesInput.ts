/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { TimelineID } from './TimelineID.ts';

export interface SearchNodesInput {
  timeline_id: TimelineID;
  pattern: string;
  limit?: number | undefined;
}
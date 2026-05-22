/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { SearchMode } from './SearchMode.ts';
import type { TimelineID } from './TimelineID.ts';

export interface SearchNodesInput {
  timeline_id: TimelineID;
  pattern?: string | undefined;
  limit?: number | undefined;
  mode?: SearchMode | undefined;
  match_properties?: { [key: string]: string } | undefined;
}
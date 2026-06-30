/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<ba8ff665c960007c9648801dfb3c9eab>>
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
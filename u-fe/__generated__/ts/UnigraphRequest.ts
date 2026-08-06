/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<845615c7d4bc91e1ea030f73b717787a>>
 */


import type { AboutGraphInput } from './AboutGraphInput.ts';
import type { ExploreGraphInput } from './ExploreGraphInput.ts';
import type { FindAncestorsInput } from './FindAncestorsInput.ts';
import type { FindPathInput } from './FindPathInput.ts';
import type { GetConfigsInput } from './GetConfigsInput.ts';
import type { GetHistoryInput } from './GetHistoryInput.ts';
import type { GraphQueryInput } from './GraphQueryInput.ts';
import type { GraphQueryMapGraphInput } from './GraphQueryMapGraphInput.ts';
import type { ListTimelinesInput } from './ListTimelinesInput.ts';
import type { PutConfigsInput } from './PutConfigsInput.ts';
import type { SearchNodesInput } from './SearchNodesInput.ts';
import type { SelectFramesInput } from './SelectFramesInput.ts';

export type UnigraphRequest =
  { "PutConfigs": PutConfigsInput } |
  { "GetConfigs": GetConfigsInput } |
  { "GraphQuery": GraphQueryInput } |
  { "GraphQueryMapGraph": GraphQueryMapGraphInput } |
  { "ListTimelines": ListTimelinesInput } |
  { "SelectFrames": SelectFramesInput } |
  { "ExploreGraph": ExploreGraphInput } |
  { "FindAncestors": FindAncestorsInput } |
  { "FindPath": FindPathInput } |
  { "SearchNodes": SearchNodesInput } |
  { "AboutGraph": AboutGraphInput } |
  { "GetHistory": GetHistoryInput };

export type UnigraphRequestVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "GraphQueryMapGraph" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "FindAncestors" | "FindPath" | "SearchNodes" | "AboutGraph" | "GetHistory";
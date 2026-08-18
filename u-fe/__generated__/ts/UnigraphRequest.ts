/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<a9ad51ee2bb360ab5d572b148482295d>>
 */


import type { AboutGraphInput } from './AboutGraphInput.ts';
import type { ExploreDeltaInput } from './ExploreDeltaInput.ts';
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
  { "ExploreDelta": ExploreDeltaInput } |
  { "FindAncestors": FindAncestorsInput } |
  { "FindPath": FindPathInput } |
  { "SearchNodes": SearchNodesInput } |
  { "AboutGraph": AboutGraphInput } |
  { "GetHistory": GetHistoryInput };

export type UnigraphRequestVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "GraphQueryMapGraph" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "ExploreDelta" | "FindAncestors" | "FindPath" | "SearchNodes" | "AboutGraph" | "GetHistory";
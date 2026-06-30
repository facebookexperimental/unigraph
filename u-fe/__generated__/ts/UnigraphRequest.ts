/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<13d5a7bfc26cdbe7df88707bbb5ce141>>
 */


import type { AboutGraphInput } from './AboutGraphInput.ts';
import type { ExploreGraphInput } from './ExploreGraphInput.ts';
import type { FindAncestorsInput } from './FindAncestorsInput.ts';
import type { FindPathInput } from './FindPathInput.ts';
import type { GetConfigsInput } from './GetConfigsInput.ts';
import type { GraphQueryInput } from './GraphQueryInput.ts';
import type { ListTimelinesInput } from './ListTimelinesInput.ts';
import type { PutConfigsInput } from './PutConfigsInput.ts';
import type { SearchNodesInput } from './SearchNodesInput.ts';
import type { SelectFramesInput } from './SelectFramesInput.ts';

export type UnigraphRequest =
  { "PutConfigs": PutConfigsInput } |
  { "GetConfigs": GetConfigsInput } |
  { "GraphQuery": GraphQueryInput } |
  { "ListTimelines": ListTimelinesInput } |
  { "SelectFrames": SelectFramesInput } |
  { "ExploreGraph": ExploreGraphInput } |
  { "FindAncestors": FindAncestorsInput } |
  { "FindPath": FindPathInput } |
  { "SearchNodes": SearchNodesInput } |
  { "AboutGraph": AboutGraphInput };

export type UnigraphRequestVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "FindAncestors" | "FindPath" | "SearchNodes" | "AboutGraph";
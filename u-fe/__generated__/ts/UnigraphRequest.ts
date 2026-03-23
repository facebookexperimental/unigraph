/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ExploreGraphInput } from './ExploreGraphInput.ts';
import type { GetConfigsInput } from './GetConfigsInput.ts';
import type { GraphQueryInput } from './GraphQueryInput.ts';
import type { ListTimelinesInput } from './ListTimelinesInput.ts';
import type { PutConfigsInput } from './PutConfigsInput.ts';
import type { SelectFramesInput } from './SelectFramesInput.ts';

export type UnigraphRequest =
  { "PutConfigs": PutConfigsInput } |
  { "GetConfigs": GetConfigsInput } |
  { "GraphQuery": GraphQueryInput } |
  { "ListTimelines": ListTimelinesInput } |
  { "SelectFrames": SelectFramesInput } |
  { "ExploreGraph": ExploreGraphInput };

export type UnigraphRequestVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "ListTimelines" | "SelectFrames" | "ExploreGraph";
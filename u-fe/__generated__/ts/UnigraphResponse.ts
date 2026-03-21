/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GetConfigsOutput } from './GetConfigsOutput.ts';
import type { GraphQueryOutput } from './GraphQueryOutput.ts';
import type { ListTimelinesOutput } from './ListTimelinesOutput.ts';
import type { PutConfigsOutput } from './PutConfigsOutput.ts';
import type { SelectFramesOutput } from './SelectFramesOutput.ts';

export type UnigraphResponse =
  { "PutConfigs": PutConfigsOutput } |
  { "GetConfigs": GetConfigsOutput } |
  { "GraphQuery": GraphQueryOutput } |
  { "ListTimelines": ListTimelinesOutput } |
  { "SelectFrames": SelectFramesOutput };

export type UnigraphResponseVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "ListTimelines" | "SelectFrames";
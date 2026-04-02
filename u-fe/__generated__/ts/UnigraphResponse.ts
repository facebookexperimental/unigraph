/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AboutGraphOutput } from './AboutGraphOutput.ts';
import type { ExploreGraphOutput } from './ExploreGraphOutput.ts';
import type { GetConfigsOutput } from './GetConfigsOutput.ts';
import type { GraphQueryOutput } from './GraphQueryOutput.ts';
import type { ListTimelinesOutput } from './ListTimelinesOutput.ts';
import type { PutConfigsOutput } from './PutConfigsOutput.ts';
import type { RpcError } from './RpcError.ts';
import type { SearchNodesOutput } from './SearchNodesOutput.ts';
import type { SelectFramesOutput } from './SelectFramesOutput.ts';

export type UnigraphResponse =
  { "PutConfigs": PutConfigsOutput } |
  { "GetConfigs": GetConfigsOutput } |
  { "GraphQuery": GraphQueryOutput } |
  { "ListTimelines": ListTimelinesOutput } |
  { "SelectFrames": SelectFramesOutput } |
  { "ExploreGraph": ExploreGraphOutput } |
  { "SearchNodes": SearchNodesOutput } |
  { "AboutGraph": AboutGraphOutput } |
  { "Error": RpcError };

export type UnigraphResponseVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "SearchNodes" | "AboutGraph" | "Error";
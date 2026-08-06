/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<7b9f20e8c03cc169549fc1bd2596b9ab>>
 */


import type { AboutGraphOutput } from './AboutGraphOutput.ts';
import type { ExploreGraphOutput } from './ExploreGraphOutput.ts';
import type { FindAncestorsOutput } from './FindAncestorsOutput.ts';
import type { FindPathOutput } from './FindPathOutput.ts';
import type { GetConfigsOutput } from './GetConfigsOutput.ts';
import type { GetHistoryOutput } from './GetHistoryOutput.ts';
import type { GraphQueryMapGraphOutput } from './GraphQueryMapGraphOutput.ts';
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
  { "GraphQueryMapGraph": GraphQueryMapGraphOutput } |
  { "ListTimelines": ListTimelinesOutput } |
  { "SelectFrames": SelectFramesOutput } |
  { "ExploreGraph": ExploreGraphOutput } |
  { "FindAncestors": FindAncestorsOutput } |
  { "FindPath": FindPathOutput } |
  { "SearchNodes": SearchNodesOutput } |
  { "AboutGraph": AboutGraphOutput } |
  { "GetHistory": GetHistoryOutput } |
  { "Error": RpcError };

export type UnigraphResponseVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "GraphQueryMapGraph" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "FindAncestors" | "FindPath" | "SearchNodes" | "AboutGraph" | "GetHistory" | "Error";
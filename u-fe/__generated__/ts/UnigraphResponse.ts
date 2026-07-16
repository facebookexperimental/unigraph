/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<dab8410f53dd842bda3983b38e35614f>>
 */


import type { AboutGraphOutput } from './AboutGraphOutput.ts';
import type { ExploreGraphOutput } from './ExploreGraphOutput.ts';
import type { FindAncestorsOutput } from './FindAncestorsOutput.ts';
import type { FindPathOutput } from './FindPathOutput.ts';
import type { GetConfigsOutput } from './GetConfigsOutput.ts';
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
  { "Error": RpcError };

export type UnigraphResponseVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "GraphQueryMapGraph" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "FindAncestors" | "FindPath" | "SearchNodes" | "AboutGraph" | "Error";
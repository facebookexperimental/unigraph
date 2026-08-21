/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<730da84eaf89815d2c32865660da6f84>>
 */


import type { AboutGraphOutput } from './AboutGraphOutput.ts';
import type { ExploreDeltaOutput } from './ExploreDeltaOutput.ts';
import type { ExploreGraphOutput } from './ExploreGraphOutput.ts';
import type { FindAncestorsOutput } from './FindAncestorsOutput.ts';
import type { FindPathOutput } from './FindPathOutput.ts';
import type { GetConfigsOutput } from './GetConfigsOutput.ts';
import type { GetHistoryOutput } from './GetHistoryOutput.ts';
import type { GraphQueryMapGraphOutput } from './GraphQueryMapGraphOutput.ts';
import type { GraphQueryOutput } from './GraphQueryOutput.ts';
import type { ListTimelinesOutput } from './ListTimelinesOutput.ts';
import type { MinCutOutput } from './MinCutOutput.ts';
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
  { "ExploreDelta": ExploreDeltaOutput } |
  { "FindAncestors": FindAncestorsOutput } |
  { "FindPath": FindPathOutput } |
  { "MinCut": MinCutOutput } |
  { "SearchNodes": SearchNodesOutput } |
  { "AboutGraph": AboutGraphOutput } |
  { "GetHistory": GetHistoryOutput } |
  { "Error": RpcError };

export type UnigraphResponseVariants = "PutConfigs" | "GetConfigs" | "GraphQuery" | "GraphQueryMapGraph" | "ListTimelines" | "SelectFrames" | "ExploreGraph" | "ExploreDelta" | "FindAncestors" | "FindPath" | "MinCut" | "SearchNodes" | "AboutGraph" | "GetHistory" | "Error";
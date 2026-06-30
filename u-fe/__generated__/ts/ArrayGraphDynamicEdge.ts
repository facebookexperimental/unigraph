/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<601c4c31a8954e51cbd4c22968ff5258>>
 */


import type { NodeIDX } from './NodeIDX.ts';

export interface ArrayGraphDynamicEdge {
  branches: { [key: string]: NodeIDX[] };
  metadata?: { [key: string]: string } | undefined;
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<103fbc212de8b8189a922242b70703a2>>
 */


import type { DynamicEdgeInfo } from './DynamicEdgeInfo.ts';

/** A single hop in the path, with edge metadata. */
export interface PathHop {
  /** Node name at this position in the path. */
  node: string;
  /** Edge tag leading *to* this node (e.g. "lazy"). None for the first hop. */
  tag?: string | undefined;
  /** Dynamic edge info leading *to* this node. None for the first hop. */
  dynamic?: DynamicEdgeInfo | undefined;
}
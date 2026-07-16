/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<1ced9254faa53f2bbd6e633bce30fe82>>
 */


import type { NodeIDX } from './NodeIDX.ts';

/** A single edge in a [`MinCutResult`], as node indices in the UI namespace. */
export interface MinCutEdge {
  from: NodeIDX;
  to: NodeIDX;
}
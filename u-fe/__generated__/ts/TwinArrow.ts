/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Arrow } from './Arrow.ts';
import type { NodeIDX } from './NodeIDX.ts';

export interface TwinArrow {
  points_to: NodeIDX;
  points_from: NodeIDX;
  l?: Arrow | undefined;
  r?: Arrow | undefined;
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { NodeIDX } from './NodeIDX.ts';

export interface ArrayGraphSerializableNodeMetadata {
  metrics: { [key: string]: number[] };
  tag_sets: { [key: NodeIDX]: { [key: string]: string[] } };
}
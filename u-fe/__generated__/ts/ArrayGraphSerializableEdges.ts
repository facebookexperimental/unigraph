/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

import type { ArrayGraphDynamicEdge } from "./ArrayGraphDynamicEdge.ts";
import type { NodeIDX } from "./NodeIDX.ts";

export interface ArrayGraphSerializableEdges {
  directed: NodeIDX[];
  directed_offsets: number[];
  tagged: { [key: NodeIDX]: { [key: string]: NodeIDX[] } };
  dynamic: { [key: NodeIDX]: ArrayGraphDynamicEdge[] };
}

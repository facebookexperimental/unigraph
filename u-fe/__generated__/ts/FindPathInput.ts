/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<53c0743bcd1b4a59471ae4bf46cb7ace>>
 */


import type { GraphHandle } from './GraphHandle.ts';

export interface FindPathInput {
  /** Graph handle — timeline ID, graph key, or GQC key. */
  handle: GraphHandle;
  /** Starting node name. */
  from: string;
  /** Target node name. */
  to: string;
  /**
   * Nodes the path may not run through.
   * 
   * Answers "is there another way?" — given `A -> B -> C -> D`, avoiding `C`
   * returns the best path that routes around it, or reports no path when `C`
   * is the only way through.
   * 
   * These are *nodes*, where `MinCut` protects *edges*: the question here is
   * which intermediate steps are acceptable, not which specific links must
   * survive.
   */
  avoid_nodes: string[];
  /** When true, include a human-readable ASCII summary in the response. */
  include_ascii?: boolean | undefined;
}
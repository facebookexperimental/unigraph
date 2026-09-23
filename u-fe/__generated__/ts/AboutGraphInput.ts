/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<b52a5cbe4773a0d2658e543abed5c15a>>
 */


import type { GraphHandle } from './GraphHandle.ts';

export interface AboutGraphInput {
  /**
   * Graph handle: a timeline_id ("cargo"), graph_key ("cargo~356"),
   * or gqc_key ("gqc_abc123").
   */
  handle: GraphHandle;
  /**
   * When false, leave `text` empty and skip rendering it. Every other field
   * is unaffected, so a caller that only wants `properties` or `stats` can
   * stop paying to build a summary it discards.
   * 
   * **Absent means true here**, unlike the `include_ascii` on the explore
   * RPCs, which defaults to false. Those shipped with the flag; this one is
   * being added to an RPC whose `text` was unconditional, and a caller
   * predating the flag — including an older `meta` binary — still expects
   * it. Defaulting off would blank their output mid-rollout.
   */
  include_ascii?: boolean | undefined;
}
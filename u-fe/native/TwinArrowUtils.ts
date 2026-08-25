// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { TwinArrow } from "../__generated__/ts/TwinArrow";

/// How many unchanged nodes "changed nodes only" mode collapsed on the way to
/// this row. `0` outside that mode, where every edge is a direct one.
///
/// A one-sided arrow still collapsed a path: when a node was added, only the
/// right graph has an edge leading to it, and that edge carries the count. So
/// the missing side is not a zero to be minimised against — it is absent, and
/// the side that exists is the answer. Mirrors `ExploreDeltaArrow::skipped` in
/// `unigraph_app/src/rpc_req/explore_delta/rows.rs`.
export function skippedNodeCount(twinArrow: TwinArrow): number {
  const l = twinArrow.l?.skipped;
  const r = twinArrow.r?.skipped;

  if (l != null && r != null) {
    return Math.min(l, r);
  }
  return l ?? r ?? 0;
}

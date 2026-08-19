/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<84b78d7d687be8b757a11096e54db115>>
 */


import type { ExploreDeltaEdge } from './ExploreDeltaEdge.ts';

/**
 * One row of a delta table — `ExploreGraphArrow`'s `name` + `metrics`, plus
 * what a comparison needs: how the node changed, the edge as it exists on each
 * side, and how many unchanged nodes were collapsed to get here.
 * 
 * `tag` / `dynamic` live inside `l` and `r` rather than on the row, because an
 * edge can be retagged while both endpoints stay identical.
 */
export interface ExploreDeltaArrow {
  /** Node name. */
  name: string;
  /**
   * Flat metrics map, keyed by `MetricView` display strings — e.g.
   * `size~transitive` (right graph), `size~transitive@left`,
   * `size~transitive@delta`.
   */
  metrics: { [key: string]: number };
  /**
   * What changed about this node between the two graphs. Bitflags:
   * 1 = added, 2 = removed, 4 = edges changed, 8 = metrics changed.
   * 
   * This describes the *node*. An edge that was added, removed, or retagged
   * shows up in `l` / `r` instead — `node_diff` would put that on the edge's
   * source, which isn't this row.
   */
  node_diff: number;
  /**
   * The edge leading here in the "before" graph. `None` means the edge is
   * new, or that this row isn't reached via an edge (entry points, all-nodes).
   */
  l?: ExploreDeltaEdge | undefined;
  /**
   * The edge leading here in the "after" graph. `None` means the edge was
   * removed, or that this row isn't reached via an edge.
   */
  r?: ExploreDeltaEdge | undefined;
  /**
   * How many unchanged nodes were collapsed on the way to this one.
   * 0 means a direct edge; always 0 unless `changed_nodes_only`.
   */
  skipped: number;
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<cc3aa06cca316aad329ac90cadc9e95d>>
 */


import type { NodeSelection } from './NodeSelection.ts';
import type { TimelineID } from './TimelineID.ts';

/**
 * Find nodes matching a [`NodeSelection`] — by name, properties, or edge tags.
 * 
 * The name mode defaults to `Substring`. Typeahead callers that want the
 * subsequence, shortest-first behaviour have to ask for `Fuzzy` explicitly.
 */
export interface SearchNodesInput {
  timeline_id: TimelineID;
  /** Which nodes to match. An empty selection matches every node. */
  selection: NodeSelection;
  /**
   * Maximum number of matches to return. Defaults to 30.
   * 
   * Under the `Fuzzy` name mode this is the top-K cap, so the result is the
   * best `limit` matches rather than a page of a larger set.
   */
  limit?: number | undefined;
}
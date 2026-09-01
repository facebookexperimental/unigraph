/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<fca6893d532c9af6596eb3e427f640f6>>
 */


import type { DynamicEdgeInfo } from './DynamicEdgeInfo.ts';

export interface ExploreGraphArrow {
  /** Node name. */
  name: string;
  /**
   * Flat metrics map. Keys follow naming conventions:
   * - "{metric}" — self value
   * - "{metric}_transitive" — transitive sum
   * - "{metric}_dominated" — dominated sum
   * - "{metric}_{tier}" — tiered transitive (if tiers configured)
   * - "parents_count" — number of configured parents
   * - "children_count" — number of children in current graph structure
   */
  metrics: { [key: string]: number };
  /** Edge tag (e.g. "lazy"), if this is a tagged edge. */
  tag?: string | undefined;
  /** Dynamic edge info, if this is a dynamic edge. */
  dynamic?: DynamicEdgeInfo | undefined;
  /**
   * True when the traversal did not follow this edge. A property of the
   * *edge* — the node it points to may still be reachable by another path.
   * Only ever true when `include_excluded` was requested.
   * 
   * `default` so a client built against this schema can still decode a
   * response from a service that predates the field.
   */
  excluded: boolean;
  /**
   * True when the node this arrow points to is not reachable from the entry
   * points at all. A property of the *node*, so it can be true even for an
   * edge that was followed (when the parent is itself unreachable).
   */
  unreachable: boolean;
  /**
   * Why the traversal skipped this edge, e.g. "tag `lazy` is above max
   * tier". Only the winning rule is recorded, not a full audit trail.
   */
  exclusion_reason?: string | undefined;
}
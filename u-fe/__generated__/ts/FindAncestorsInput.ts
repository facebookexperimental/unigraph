/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


export interface FindAncestorsInput {
  /** Graph handle — timeline ID, graph key, or GQC key. */
  handle: string;
  /** The node to find ancestors of. */
  node_name: string;
  /** Property predicates — all must match (AND). e.g. `{"type": "budget"}`. */
  properties?: { [key: string]: string } | undefined;
  /** When true, only return ancestors with no parents (graph entrypoints). */
  parentless?: boolean | undefined;
  /** Skip first N matching results (for pagination). Defaults to 0. */
  offset?: number | undefined;
  /** Maximum number of results to return. Defaults to 100. */
  limit?: number | undefined;
  /** When true, include a human-readable ASCII summary in the response. */
  include_ascii?: boolean | undefined;
}
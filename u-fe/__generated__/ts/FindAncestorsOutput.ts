/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


export interface FindAncestorsOutput {
  /** Matching ancestor node names (paginated). */
  ancestors: string[];
  /** Total number of matching ancestors (before offset/limit). */
  total_count: number;
  /** Human-readable summary. Only populated when `include_ascii` is true. */
  ascii?: string | undefined;
}
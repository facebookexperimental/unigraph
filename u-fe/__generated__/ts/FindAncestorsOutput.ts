/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<4b0656f6f6b954802217f0fe26d8578f>>
 */


export interface FindAncestorsOutput {
  /** Matching ancestor node names (paginated). */
  ancestors: string[];
  /** Total number of matching ancestors (before offset/limit). */
  total_count: number;
  /** Human-readable summary. Only populated when `include_ascii` is true. */
  ascii?: string | undefined;
}
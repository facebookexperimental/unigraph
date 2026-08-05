/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f90c233002b30f8ea009e0246362c1c1>>
 */


export interface PropertyCandidates {
  name: string;
  /** Distinct values, ascending. Empty when `high_cardinality`. */
  values: string[];
  /**
   * The property has more distinct values than the UI can usefully offer,
   * so they were not collected and the input should accept freeform text.
   */
  high_cardinality: boolean;
}
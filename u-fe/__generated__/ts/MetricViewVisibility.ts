/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/** Controls when a metric view column is shown in the UI. */
export type MetricViewVisibility =
  /** Show when the relevant global toggle is on. */
  { "Enabled": {  } } |
  /** Show only in dominator graph structure mode (and global toggle is on). */
  { "EnabledInDominatorMode": {  } } |
  /** Never show. */
  { "Hidden": {  } } |
  /** Not available — nonsensical combination (e.g. "size_transitive~transitive"). */
  { "Unavailable": { reason: string } };

export type MetricViewVisibilityVariants = "Enabled" | "EnabledInDominatorMode" | "Hidden" | "Unavailable";
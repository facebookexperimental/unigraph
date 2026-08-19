/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<b7f7deb1ef1ef66bff463651c537d296>>
 */


export type SortColumn =
  /** Sort by node name (tree column) */
  { "NodeName": {  } } |
  /**
   * Sort by a metric view column.
   * 
   * The key is a `MetricView` string, optionally suffixed with `@left` or
   * `@delta`. A bare key means the right-hand graph — which is the only
   * graph outside delta mode, so every single-graph key is valid here:
   * 
   * ```text
   * size#eager              sort by the eager-tier size
   * size~transitive@left    sort by the before graph's transitive size
   * size~transitive@delta   sort by how much it changed
   * ```
   * 
   * Serialized as that string, so this stays wire-compatible with the
   * stored graph settings that predate the typed representation.
   */
  { "MetricView": { key: string } };

export type SortColumnVariants = "NodeName" | "MetricView";
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


export type SortColumn =
  /** Sort by node name (tree column) */
  { "NodeName": {  } } |
  /**
   * Sort by a metric view column. `key` is `MetricView.to_string()`,
   * optionally suffixed with `@right` or `@delta` to select the
   * comparison-graph or delta column in twin-graph mode.
   */
  { "MetricView": { key: string } };

export type SortColumnVariants = "NodeName" | "MetricView";
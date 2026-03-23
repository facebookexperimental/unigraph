/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Specifies which metric to compute for each arrow.
 * Each variant maps to a key in the flat `metrics` map on `ExploreGraphArrow`.
 */
export type NodeMetric =
  /** Self metric value. Key: `"{name}"`. */
  { "Metric": { name: string } } |
  /** Transitive metric sum (DFS over forward edges). Key: `"{name}_transitive"`. */
  { "MetricTransitive": { name: string } } |
  /** Dominated metric sum (DFS over dominator tree). Key: `"{name}_dominated"`. */
  { "MetricDominated": { name: string } } |
  /** Tiered transitive metric (cumulative at tier). Key: `"{name}_{tier}"`. */
  { "MetricTiered": { name: string, tier: string } } |
  /** Number of configured parents. Key: `"parents_count"`. */
  { "ParentsCount": {  } } |
  /** Number of children in current graph structure. Key: `"children_count"`. */
  { "ChildrenCount": {  } } |
  /** Transitive dependency count (forward DFS). Key: `"count_transitive"`. */
  { "CountTransitive": {  } } |
  /** Dominated dependency count (dominator tree DFS). Key: `"count_dominated"`. */
  { "CountDominated": {  } };

export type NodeMetricVariants = "Metric" | "MetricTransitive" | "MetricDominated" | "MetricTiered" | "ParentsCount" | "ChildrenCount" | "CountTransitive" | "CountDominated";
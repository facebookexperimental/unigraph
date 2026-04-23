/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * A user-facing metric specification.
 * 
 * Describes which metric to compute for a node. Not the raw data itself,
 * but the *view* — plain value, transitive sum, dominated sum, tiered, or
 * a structural count like parent count or transitive node count.
 * 
 * ## String format
 * 
 * `MetricView` implements `Display` and `FromStr`. The `~` separator
 * separates the metric name from the view variant, while `#` introduces
 * a tier name:
 * 
 * ```text
 * size                  → Metric { name: "size" }
 * size~transitive       → Transitive { name: "size" }
 * size~dominated        → Dominated { name: "size" }
 * size#T1               → Tiered { name: "size", tier_name: "T1" }
 * size#T1~dominated     → TieredDominated { name: "size", tier_name: "T1" }
 * node-count~transitive → CountTransitive
 * node-count~dominated  → CountDominated
 * parents-count         → ParentsCount
 * ```
 */
export type MetricView =
  /** Raw metric value (e.g. file size in bytes). */
  { "Metric": { name: string } } |
  /** Transitive metric sum (DFS over forward edges). */
  { "Transitive": { name: string } } |
  /** Dominated metric sum (DFS over dominator tree). */
  { "Dominated": { name: string } } |
  /** Tiered transitive metric (cumulative at a specific tier). */
  { "Tiered": { name: string, tier_name: string } } |
  /** Tiered dominated metric (dominated sum at a specific tier). */
  { "TieredDominated": { name: string, tier_name: string } } |
  /** Number of configured parents (incoming edges). */
  { "ParentsCount": {  } } |
  /** Transitive dependency count (forward DFS). */
  { "CountTransitive": {  } } |
  /** Dominated dependency count (dominator tree DFS). */
  { "CountDominated": {  } };

export type MetricViewVariants = "Metric" | "Transitive" | "Dominated" | "Tiered" | "TieredDominated" | "ParentsCount" | "CountTransitive" | "CountDominated";
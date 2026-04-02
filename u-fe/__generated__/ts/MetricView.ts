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
 * `MetricView` implements `Display` and `FromStr` using `~` as a separator
 * between the metric name and the view variant:
 * 
 * ```text
 * size                  → Metric { name: "size" }
 * size~transitive       → Transitive { name: "size" }
 * size~dominated        → Dominated { name: "size" }
 * size~T1               → Tiered { name: "size", tier_name: "T1" }
 * size~dominated~T1     → TieredDominated { name: "size", tier_name: "T1" }
 * size~conjoint~T1      → ConjointTiered { name: "size", tier_name: "T1" }
 * node-count~transitive → CountTransitive
 * node-count~dominated  → CountDominated
 * node-count~conjoint   → CountConjoint
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
  /** Conjoint tiered metric (transitive cost / parent count at a specific tier). */
  { "ConjointTiered": { name: string, tier_name: string } } |
  /** Number of configured parents (incoming edges). */
  { "ParentsCount": {  } } |
  /** Transitive dependency count (forward DFS). */
  { "CountTransitive": {  } } |
  /** Dominated dependency count (dominator tree DFS). */
  { "CountDominated": {  } } |
  /** Conjoint dependency count (transitive count / parent count). */
  { "CountConjoint": {  } };

export type MetricViewVariants = "Metric" | "Transitive" | "Dominated" | "Tiered" | "TieredDominated" | "ConjointTiered" | "ParentsCount" | "CountTransitive" | "CountDominated" | "CountConjoint";
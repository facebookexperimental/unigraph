/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0127df475ffdccacdd3a9e3e59d23316>>
 */


import type { MetricSide } from './MetricSide.ts';

/**
 * A user-facing metric specification.
 * 
 * Describes which metric to compute for a node. Not the raw data itself,
 * but the *view* — plain value, transitive sum, dominated sum, tiered, or
 * a structural count like parent count or transitive node count.
 * 
 * ## Sides
 * 
 * Every view carries an optional [`MetricSide`]. `None` — the single-graph
 * case, and what JSON gets when the field is simply omitted — means the
 * primary graph; `Some(Left)` / `Some(Delta)` only mean anything when a table
 * is comparing two. Keeping it here rather than in a separate wrapper type
 * means one vocabulary covers both modes: sort keys, RPC column lists, and
 * the metrics map are all just `MetricView`.
 * 
 * ## String format
 * 
 * `MetricView` implements `Display` and `FromStr`. `~` separates the metric
 * name from the view variant, `#` introduces a tier name, and `@` the side:
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
 * tier                  → TierIndex
 * 
 * size~transitive@left  → Transitive { name: "size", side: Some(Left) }
 * size#T1@delta         → Tiered { name: "size", tier_name: "T1", side: Some(Delta) }
 * ```
 * 
 * ## Legacy forms
 * 
 * Tier names were once introduced by `~` rather than `#`. Those keys are
 * persisted inside `GraphSettings` on stored graphs, so both spellings parse;
 * `Display` always emits the current one, which migrates a key on rewrite.
 * 
 * ```text
 * size~T1               → Tiered { name: "size", tier_name: "T1" }
 * size~dominated~T1     → TieredDominated { name: "size", tier_name: "T1" }
 * ```
 */
export type MetricView =
  /** Raw metric value (e.g. file size in bytes). */
  { "Metric": { name: string, side?: MetricSide | undefined } } |
  /** Transitive metric sum (DFS over forward edges). */
  { "Transitive": { name: string, side?: MetricSide | undefined } } |
  /** Dominated metric sum (DFS over dominator tree). */
  { "Dominated": { name: string, side?: MetricSide | undefined } } |
  /** Tiered transitive metric (cumulative at a specific tier). */
  { "Tiered": { name: string, tier_name: string, side?: MetricSide | undefined } } |
  /** Tiered dominated metric (dominated sum at a specific tier). */
  { "TieredDominated": { name: string, tier_name: string, side?: MetricSide | undefined } } |
  /** Number of configured parents (incoming edges). */
  { "ParentsCount": { side?: MetricSide | undefined } } |
  /** Transitive dependency count (forward DFS). */
  { "CountTransitive": { side?: MetricSide | undefined } } |
  /** Dominated dependency count (dominator tree DFS). */
  { "CountDominated": { side?: MetricSide | undefined } } |
  /** Tier index of the node (0-based). Only available when tiers are configured. */
  { "TierIndex": { side?: MetricSide | undefined } };

export type MetricViewVariants = "Metric" | "Transitive" | "Dominated" | "Tiered" | "TieredDominated" | "ParentsCount" | "CountTransitive" | "CountDominated" | "TierIndex";
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AscendingTiersConfig } from './AscendingTiersConfig.ts';

/**
 * Configuration for tiered traversal, which allows traversing the graph in tiers.
 * Specific use case for this is JavaScript loading tiers. E.g. initial payload vs.
 * lazyloaded JS.
 * When we traverse the graph we look at the tagged edges. If the edge has a tag
 * we look at the node's current tier and then we look at the new tier this node
 * is supposed to transition to and record that.
 */
export type TieredTraversalConfig =
  { "AscendingTiers": AscendingTiersConfig };

export type TieredTraversalConfigVariants = "AscendingTiers";
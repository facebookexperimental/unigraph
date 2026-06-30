/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<1a91316116bfcba57bdc87c82768a56e>>
 */


import type { AscendingTier } from './AscendingTier.ts';

export interface AscendingTiersConfig {
  tiers: AscendingTier[];
  /**
   * If this is set, the traversal will stop at this tier
   * and not traverse any further.
   */
  max_tier?: number | undefined;
}
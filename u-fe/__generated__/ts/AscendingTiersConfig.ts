/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

import type { AscendingTier } from "./AscendingTier.ts";

export interface AscendingTiersConfig {
  tiers: AscendingTier[];
  /**
   * If this is set, the traversal will stop at this tier
   * and not traverse any further.
   */
  max_tier?: number | undefined;
}

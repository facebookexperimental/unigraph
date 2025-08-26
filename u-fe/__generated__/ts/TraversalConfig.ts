/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

import type { Decision } from "./Decision.ts";
import type { ForceDynamic } from "./ForceDynamic.ts";
import type { Message } from "./Message.ts";
import type { NodeTagSetsPredicate } from "./NodeTagSetsPredicate.ts";
import type { TieredTraversalConfig } from "./TieredTraversalConfig.ts";

export interface TraversalConfig {
  force_nodes?: { [key: string]: Decision } | undefined;
  /** From Node Name -> To Node Name -> Decision */
  force_edges?: { [key: string]: { [key: string]: Decision } } | undefined;
  /** Only applied to tagged edges */
  force_tagged?: { [key: string]: Decision } | undefined;
  /** These rules are ordered. The first one that matches will be used. */
  tag_sets?: NodeTagSetsPredicate[] | undefined;
  /** These rules are ordered. The first one that matches will be used. */
  force_dynamic?: ForceDynamic[] | undefined;
  tiered_traversal?: TieredTraversalConfig | undefined;
  messages?: { [key: string]: Message } | undefined;
}

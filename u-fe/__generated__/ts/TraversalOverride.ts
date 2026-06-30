/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<1c0869454d3e6c161f03bb4191151a6c>>
 */


import type { TraversalConfig } from './TraversalConfig.ts';
import type { TraversalConfigKey } from './TraversalConfigKey.ts';

/** How to override the traversal config for an explored graph. */
export type TraversalOverride =
  /** Full inline traversal config. */
  { "Inline": TraversalConfig } |
  /** Reference to a stored traversal config by key. */
  { "Key": TraversalConfigKey };

export type TraversalOverrideVariants = "Inline" | "Key";
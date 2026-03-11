/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Decision } from './Decision.ts';
import type { DynamicTypeConfig } from './DynamicTypeConfig.ts';
import type { Message } from './Message.ts';
import type { NodeLabelPredicate } from './NodeLabelPredicate.ts';
import type { TieredTraversalConfig } from './TieredTraversalConfig.ts';

export interface TraversalConfig {
  force_nodes?: { [key: string]: Decision } | undefined;
  /** From Node Name -> To Node Name -> Decision */
  force_edges?: { [key: string]: { [key: string]: Decision } } | undefined;
  /** Only applied to tagged edges */
  force_tagged?: { [key: string]: Decision } | undefined;
  label_predicates?: { [key: string]: NodeLabelPredicate } | undefined;
  force_dynamic?: { [key: string]: DynamicTypeConfig } | undefined;
  tiered_traversal?: TieredTraversalConfig | undefined;
  messages?: { [key: string]: Message } | undefined;
}
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Decision } from './Decision.ts';
import type { ForceDynamic } from './ForceDynamic.ts';
import type { Message } from './Message.ts';
import type { NodeTagSetsPredicate } from './NodeTagSetsPredicate.ts';
import type { TieredTraversalConfig } from './TieredTraversalConfig.ts';

export interface TraversalConfig {
  force_nodes: { [key: string]: Decision };
  /** From Node Name -> To Node Name -> Decision */
  force_edges: { [key: string]: { [key: string]: Decision } };
  /**
   * This will force all nodes that are children of the given node.
   * This is useful for cases where you want to exclude all imports
   * of a specific node (like `MySharedInfraModules.js`) with a single
   * config.
   */
  force_children_of: { [key: string]: Decision };
  /** Only applied to tagged edges */
  force_tagged: { [key: string]: Decision };
  /** These rules are ordered. The first one that matches will be used. */
  tag_sets: NodeTagSetsPredicate[];
  /** These rules are ordered. The first one that matches will be used. */
  force_dynamic: ForceDynamic[];
  tiered_traversal?: TieredTraversalConfig | undefined;
  messages?: { [key: string]: Message } | undefined;
}
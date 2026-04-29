/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphNode } from './GraphNode.ts';
import type { GraphSettings } from './GraphSettings.ts';
import type { TraversalConfig } from './TraversalConfig.ts';

export interface MapGraph {
  nodes: { [key: string]: GraphNode };
  traversal_config?: TraversalConfig | undefined;
  graph_settings?: GraphSettings | undefined;
  /**
   * If present, these graph will use these entry points instead
   * of automatically determining them.
   */
  entry_points?: string[] | undefined;
  /** Graph-level key-value properties (not per-node). */
  properties: { [key: string]: string };
}
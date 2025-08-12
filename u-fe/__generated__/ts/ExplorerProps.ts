/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { CallbackFn } from './CallbackFn.ts';
import type { ExplorerComponentInputGraphs } from './ExplorerComponentInputGraphs.ts';

export interface ExplorerProps {
  /**
   * NODE: DO NOT FORGET TO MEMOIZE IF YOU CONSTRUCT THIS OBJECT.
   * 
   * Provide a graph to visualize/explore. Can be a single graph
   * or two graphs that will be compared to each other.
   */
  graphs: ExplorerComponentInputGraphs;
  /**
   * serialized traversal config. Serialization format
   * 1. JSON
   * 2. ZSTD compression
   * 3. Base64 (UrlSafe, NoPadding)
   */
  traversal_config?: string | undefined;
  on_traversal_config_change: CallbackFn;
  /**
   * serialized traversal config. Serialization format
   * 1. JSON
   * 2. ZSTD compression
   * 3. Base64 (UrlSafe, NoPadding)
   */
  graph_settings?: string | undefined;
  on_graph_settings_change: CallbackFn;
}
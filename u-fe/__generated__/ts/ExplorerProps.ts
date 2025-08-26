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
   * serialized traversal config (for the Left graph).
   * Serialization format:
   * 1. JSON
   * 2. ZSTD compression
   * 3. Base64 (UrlSafe, NoPadding)
   */
  traversal_config_l?: string | undefined;
  on_traversal_config_change_l?: CallbackFn | undefined;
  /**
   * Same as traversal config, but for the Right graph
   * (in delta/comparison view)
   */
  traversal_config_r?: string | undefined;
  on_traversal_config_change_r?: CallbackFn | undefined;
  /**
   * serialized traversal config. Serialization format
   * 1. JSON
   * 2. ZSTD compression
   * 3. Base64 (UrlSafe, NoPadding)
   */
  graph_settings?: string | undefined;
  on_graph_settings_change: CallbackFn;
}
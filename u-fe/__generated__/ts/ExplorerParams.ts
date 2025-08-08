/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { CallbackFn } from './CallbackFn.ts';
import type { ExplorerComponentInputGraph } from './ExplorerComponentInputGraph.ts';

export interface ExplorerParams {
  graph_left: ExplorerComponentInputGraph;
  graph_right?: ExplorerComponentInputGraph | undefined;
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
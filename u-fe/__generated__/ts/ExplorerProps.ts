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
   * Base GraphQueryConfig as JSON (from API response, immutable).
   * Used as the baseline for delta computation.
   */
  base_gqc_l?: string | undefined;
  base_gqc_r?: string | undefined;
  /**
   * GQC delta (zstd+base64) — only the fields the user changed
   * relative to the base GQC. Stored in the URL.
   */
  gqc_delta_l?: string | undefined;
  on_gqc_delta_change_l?: CallbackFn | undefined;
  gqc_delta_r?: string | undefined;
  on_gqc_delta_change_r?: CallbackFn | undefined;
  /** Serialized graph settings (zstd+base64). */
  graph_settings?: string | undefined;
  on_graph_settings_change: CallbackFn;
  /**
   * If set, the sidebar shows a home icon linking to this URL.
   * Omit for standalone/local mode where there's no home page.
   */
  home_href?: string | undefined;
}
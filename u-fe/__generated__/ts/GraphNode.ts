/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { DynamicEdge } from './DynamicEdge.ts';

export interface GraphNode {
  /**
   * Single-valued string metadata.
   * e.g. { "oncall": "unigraph", "path": "html/js/..." }
   */
  properties?: { [key: string]: string } | undefined;
  /**
   * Multi-valued categorical metadata.
   * e.g. { "AssertHasteProject": {"comet_pkg", "gemini_pkg"} }
   */
  labels?: { [key: string]: string[] } | undefined;
  metrics?: { [key: string]: number } | undefined;
  edges_directed?: string[] | undefined;
  edges_tagged?: { [key: string]: string[] } | undefined;
  edges_dynamic?: { [key: string]: { [key: string]: DynamicEdge } } | undefined;
}
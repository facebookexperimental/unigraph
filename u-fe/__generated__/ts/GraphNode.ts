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
   * 
   * **Performance note:** Properties are stored as heap-allocated JSON
   * (`BTreeMap<String, String>`) and converted to per-property sparse maps
   * in the ArrayGraph. They require pointer chasing and are significantly
   * more expensive than metrics or edges. Avoid storing information that
   * is derivable from the node name or graph structure (e.g. don't store
   * `"path"` if the node name already contains the path). Prefer encoding
   * categorical data as graph structure (entry-point nodes + edges) over
   * properties when the data can be derived by traversal.
   */
  properties?: { [key: string]: string } | undefined;
  /**
   * Multi-valued categorical metadata.
   * e.g. { "AssertHasteProject": {"comet_pkg", "gemini_pkg"} }
   * 
   * **Performance note:** Labels are the most expensive per-node field.
   * Each label is a `BTreeSet<String>` of heap-allocated strings, stored
   * in a `BTreeMap` — nested heap allocations with poor cache locality.
   * In the ArrayGraph, labels become sparse maps of `NodeIDX → BTreeSet`.
   * A single widely-shared label (e.g. route membership with 70+ values
   * on every shared module) can dominate serialization size and memory.
   * Prefer encoding membership as graph structure: create synthetic group
   * nodes with edges to members, then derive membership by reverse DFS.
   */
  labels?: { [key: string]: string[] } | undefined;
  /**
   * Numeric per-node values (e.g. file size in bytes).
   * Cheap: stored as flat `Vec<f32>` per metric in the ArrayGraph.
   */
  metrics?: { [key: string]: number } | undefined;
  /** Untagged directed edges. Cheap: stored in CSR (flat array + offsets). */
  edges_directed?: string[] | undefined;
  /**
   * Tagged directed edges. Cheap: same CSR storage, with an EdgeMeta
   * entry per tag.
   */
  edges_tagged?: { [key: string]: string[] } | undefined;
  edges_dynamic?: { [key: string]: { [key: string]: DynamicEdge } } | undefined;
}